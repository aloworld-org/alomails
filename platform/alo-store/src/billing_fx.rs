//! Foreign-exchange arithmetic for billing documents (alo Billing, ADR 0035,
//! wave B1.21): what a rate *is*, and how an amount crosses from the currency a
//! document was raised in to the currency its issuer keeps books in.
//!
//! This module is **pure** — no database, no clock, no tenant — for the same
//! reason [`crate::billing_totals`] is: the convention has to be property-
//! testable on its own, and every surface that restates an amount (the VAT
//! summary, the printed document, the PDF, the e-invoice of B1.22) has to get
//! the identical answer.
//!
//! ## A rate is an integer, quoted the way the ECB quotes it
//!
//! A rate is held as **micro-units of the quoted currency per one unit of the
//! base currency** — `1 EUR = 1.162600 USD` is `1_162_600`. Two decisions are
//! packed into that sentence:
//!
//! - **Integer, never floating point.** A rate multiplies money, so a rate that
//!   is "about" right produces money that is "about" right. Six decimal places
//!   is what the published reference rates carry (five significant figures on a
//!   near-unity pair, and the yen quoted to two) with room to spare.
//! - **Base-per-unit, not unit-per-base.** The ECB publishes one euro against
//!   everything, and keeping that direction preserves the precision of the far
//!   currencies: `1 EUR = 163.400000 JPY` is six honest decimals, whereas its
//!   reciprocal (`0.006120` EUR per yen) is four digits of a number that then
//!   multiplies every amount on the document. Crossing *down* a rate is a
//!   division ([`convert_cents`]), which is where the rounding belongs.
//!
//! ## Crossing an amount
//!
//! ```text
//! base_cents = round(doc_cents × 1_000_000 / rate_micro)
//! ```
//!
//! Computed in `i128` and rounded **half away from zero**, the one rounding
//! convention this module shares with [`crate::billing_totals`] — so that a
//! credit note stays the exact mirror of the invoice it credits *after*
//! conversion too, and a corrected period still sums to zero in the base
//! currency rather than leaving a stray cent.
//!
//! Because both sides of the division are minor units of their own currency
//! (hundredths, always, in this store), the hundredths cancel and the rate can
//! be applied to cents directly. A currency whose ISO minor unit is not a
//! hundredth (JPY, ISK) is *stored* in hundredths like every other, so its
//! conversion is right here and its **display** rounding is a separate question
//! (flagged for B1.22, where the e-invoice states the exponent).
//!
//! ## Converting a document, not an amount
//!
//! [`convert_totals`] restates a whole document by crossing **each VAT-rate
//! subtotal**, then summing — never by crossing the document's own total. The
//! reason is the one the VAT summary already lives by: a return is filed per
//! rate, so the per-rate figures are what must add up to the total, and
//! converting the total separately would let the rows disagree with it by a
//! cent.

use crate::billing_totals::{Totals, VatSubtotal, div_round_half_away, to_i64};
use crate::error::{Result, StoreError};

/// How many micro-units one whole unit of a currency is: rates are held to six
/// decimal places.
pub const RATE_SCALE: i64 = 1_000_000;

/// The rate of a currency against itself — the identity, used for a document
/// raised in the currency its issuer already keeps books in.
pub const IDENTITY_RATE_MICRO: i64 = RATE_SCALE;

/// The largest rate accepted, in micro-units: one euro to a billion units of
/// the other currency.
///
/// A typo guard with an arithmetic job, like
/// [`crate::billing_field::UNIT_PRICE_MAX_CENTS`]. Real quotes reach the
/// millions on a collapsed currency, so the ceiling is far above policy and far
/// below the point where `cents × RATE_SCALE` could leave `i128`.
pub const RATE_MICRO_MAX: i64 = 1_000_000_000 * RATE_SCALE;

/// A rate as it is stated on a document: the two currencies, the number, and
/// the day it was published.
///
/// This is the **snapshot** an issued document carries. It is deliberately a
/// value rather than a link to the rate table: a rate row could be re-imported
/// with a correction next week, and the document must keep saying what it was
/// converted at (EU VAT Directive art. 91 fixes the rate at the tax point, and
/// an auditor recomputes from the number printed on the paper).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FxSnapshot {
    /// ISO 4217 code the issuer keeps books in — what the amounts are restated
    /// *into*.
    pub base_currency: String,
    /// Micro-units of the document's currency per one unit of
    /// `base_currency`; [`IDENTITY_RATE_MICRO`] when they are the same
    /// currency.
    pub rate_micro: i64,
    /// The day the rate was published — the issue date itself when the rate is
    /// the identity, and otherwise the publication day at or before the issue
    /// date (a reference rate is not published on a weekend).
    pub rate_date: time::Date,
}

impl FxSnapshot {
    /// The snapshot of a document that needs no conversion: it is already in
    /// the base currency, on `on`.
    pub fn identity(base_currency: &str, on: time::Date) -> Self {
        Self {
            base_currency: base_currency.to_owned(),
            rate_micro: IDENTITY_RATE_MICRO,
            rate_date: on,
        }
    }

    /// Whether this snapshot restates anything — `false` for the identity, so a
    /// surface can print one figure instead of the same figure twice.
    pub fn restates(&self, document_currency: &str) -> bool {
        document_currency != self.base_currency
    }
}

/// Validates a rate in micro-units: strictly positive and within
/// [`RATE_MICRO_MAX`].
///
/// Zero is refused rather than treated as "unknown": a zero rate would turn
/// every amount on a document into a division by zero, and an absent rate is
/// [`Option::None`], not a stored zero.
///
/// # Errors
/// [`StoreError::Validation`] when the rate is not a usable number.
pub fn rate_micro(value: i64) -> Result<i64> {
    if !(1..=RATE_MICRO_MAX).contains(&value) {
        return Err(StoreError::Validation(format!(
            "an exchange rate must be between 1 and {RATE_MICRO_MAX} micro-units \
             (1 unit = {RATE_SCALE} micro-units)"
        )));
    }
    Ok(value)
}

/// Reads a rate written the way it is published — `1.162600`, `163.4`, `0.9` —
/// into micro-units.
///
/// Integer-only: the text is split at the decimal point and the two halves are
/// assembled, so a rate never passes through a float on its way into the
/// database. More than six decimal places is a refusal rather than a silent
/// truncation, because a rate nobody typed is worse than a form that asks
/// again. A thousands separator is *not* accepted: `1,234` is genuinely
/// ambiguous between a rate of one-and-a-bit and one of twelve hundred, and
/// guessing wrong misstates every amount it touches.
///
/// # Errors
/// [`StoreError::Validation`] when the text is not a plain positive decimal at
/// this scale.
pub fn parse_rate(text: &str) -> Result<i64> {
    let text = text.trim();
    let malformed = || {
        StoreError::Validation(
            "an exchange rate must be a positive decimal with at most 6 decimal places, \
             written with a '.' and no thousands separator"
                .to_owned(),
        )
    };
    if text.is_empty() {
        return Err(malformed());
    }
    let (whole, fraction) = match text.split_once('.') {
        Some((whole, fraction)) => (whole, fraction),
        None => (text, ""),
    };
    // `1.` and `.5` are typos, not shorthand; every digit must be ASCII, so no
    // Arabic-Indic numeral silently reads as a different number.
    if whole.is_empty()
        || !whole.bytes().all(|b| b.is_ascii_digit())
        || !fraction.bytes().all(|b| b.is_ascii_digit())
        || fraction.is_empty() && text.contains('.')
        || fraction.len() > 6
    {
        return Err(malformed());
    }
    let scale_digits = 6 - fraction.len();
    let whole: i64 = whole.parse().map_err(|_| malformed())?;
    let fraction: i64 = if fraction.is_empty() {
        0
    } else {
        fraction.parse().map_err(|_| malformed())?
    };
    let micro = whole
        .checked_mul(RATE_SCALE)
        .and_then(|units| units.checked_add(fraction * 10_i64.pow(scale_digits as u32)))
        .ok_or_else(malformed)?;
    // Zero answers in the words the caller typed in — "a positive decimal" —
    // rather than in micro-units, which are not what anybody wrote. The ceiling
    // keeps [`rate_micro`]'s own message, which is where the number matters.
    if micro == 0 {
        return Err(malformed());
    }
    rate_micro(micro)
}

/// A rate in micro-units as it is read and printed: a plain decimal with a `.`
/// separator, trailing zeros dropped, and at least one decimal place so it is
/// unmistakably a rate (`1_162_600` → `1.1626`, `1_000_000` → `1.0`).
///
/// Integer-only, like [`crate::billing_field`]'s amounts: the micro-units are
/// split into whole units and millionths and printed, never divided.
pub fn format_rate(micro: i64) -> String {
    let sign = if micro < 0 { "-" } else { "" };
    let abs = i128::from(micro).abs();
    let whole = abs / i128::from(RATE_SCALE);
    let fraction = abs % i128::from(RATE_SCALE);
    let digits = format!("{fraction:06}");
    let trimmed = digits.trim_end_matches('0');
    if trimmed.is_empty() {
        format!("{sign}{whole}.0")
    } else {
        format!("{sign}{whole}.{trimmed}")
    }
}

/// Crosses `cents` from the document's currency into the base currency at
/// `rate_micro`, rounded half away from zero.
///
/// Returns `None` for a rate that cannot divide (zero or negative), so a caller
/// holding a corrupt snapshot reports "not converted" rather than a figure.
pub fn convert_cents(cents: i64, rate_micro: i64) -> Option<i64> {
    if rate_micro <= 0 {
        return None;
    }
    // The identity is answered exactly rather than divided, so a document in
    // its own base currency is bit-for-bit itself.
    if rate_micro == IDENTITY_RATE_MICRO {
        return Some(cents);
    }
    let exact = i128::from(cents) * i128::from(RATE_SCALE);
    Some(to_i64(div_round_half_away(exact, i128::from(rate_micro))))
}

/// Restates a whole document's totals in the base currency at `rate_micro`.
///
/// **Each VAT-rate subtotal is crossed, and the totals are the sum of the
/// crossed rows** — never the document's total crossed on its own. A VAT return
/// is filed per rate, so the rows are the figures that must add up to the
/// total; converting the total separately would let a return disagree with its
/// own breakdown by a cent.
///
/// Returns `None` for an unusable rate, exactly as [`convert_cents`] does.
pub fn convert_totals(totals: &Totals, rate_micro: i64) -> Option<Totals> {
    // Checked before the loop, not inside it: a document with no lines has no
    // subtotal to fail on, and "nothing, converted at an impossible rate" must
    // still be a refusal rather than a row of zeros.
    if rate_micro <= 0 {
        return None;
    }
    let mut net: i128 = 0;
    let mut vat: i128 = 0;
    let mut vat_by_rate = Vec::with_capacity(totals.vat_by_rate.len());
    for subtotal in &totals.vat_by_rate {
        let row = VatSubtotal {
            rate_bp: subtotal.rate_bp,
            net_cents: convert_cents(subtotal.net_cents, rate_micro)?,
            vat_cents: convert_cents(subtotal.vat_cents, rate_micro)?,
        };
        net += i128::from(row.net_cents);
        vat += i128::from(row.vat_cents);
        vat_by_rate.push(row);
    }
    // A document may carry net at no rate at all only if it has no lines; then
    // both sums are zero, which is what its own totals say too. Anything the
    // rows do not explain (they always explain everything: every line belongs
    // to exactly one rate) would show up here as a difference, so the net is
    // taken from the rows and the guard is the assertion below.
    debug_assert_eq!(
        totals.net_cents,
        totals.vat_by_rate.iter().map(|r| r.net_cents).sum::<i64>(),
        "a document's net is exactly its per-rate rows"
    );
    Some(Totals {
        net_cents: to_i64(net),
        vat_cents: to_i64(vat),
        gross_cents: to_i64(net + vat),
        vat_by_rate,
    })
}

/// A document's money as the issuer's books see it: `None` when there is
/// nothing to restate (the document is already in the accounting currency, or it
/// carries no snapshot at all), and otherwise its totals crossed at the rate
/// frozen on it.
///
/// The one place that decision is made, so a list entry, a document view, a
/// printed page and a PDF all show a restated figure under exactly the same
/// conditions.
pub fn restated(currency: &str, fx: Option<&FxSnapshot>, totals: &Totals) -> Option<Totals> {
    let fx = fx?;
    if !fx.restates(currency) {
        return None;
    }
    convert_totals(totals, fx.rate_micro)
}

/// A document's money **as one set of books sees it**: its totals crossed into
/// `base` at the rate frozen on it, or `None` when that cannot be done
/// honestly — no snapshot at all, a snapshot taken against a different
/// accounting currency, or an unusable rate.
///
/// The difference from [`restated`] is deliberate and is the difference between
/// *printing* and *adding up*. A surface printing one document asks "is there a
/// second figure to show?", so the identity answers `None`. A report adding many
/// documents together asks "what is this worth in the currency I am summing in?",
/// and for a document already in that currency the honest answer is itself —
/// which is what this returns. Both the VAT summary and the Insights query
/// engine sum through here, so a tile and a return can never disagree about
/// which documents were converted.
pub fn restated_into(base: &str, fx: Option<&FxSnapshot>, totals: &Totals) -> Option<Totals> {
    let fx = fx?;
    if fx.base_currency != base {
        return None;
    }
    convert_totals(totals, fx.rate_micro)
}

/// **One amount** as one set of books sees it: `cents`, stated in `currency`,
/// expressed in `base` — or `None` when that cannot be done honestly.
///
/// The scalar sibling of [`restated_into`], for a report that adds up what is
/// *open* on documents rather than what they are worth (`crate::fin_aged`):
/// an open balance is one figure, not a set of per-rate subtotals, and crossing
/// it once is the whole conversion.
///
/// It differs from [`restated_into`] in one deliberate way: **a document already
/// in `base` needs no snapshot**, because nothing is being crossed. That case is
/// answered by the currency itself, which is what lets a bill — which carries no
/// snapshot at all, having been written by somebody else's system
/// (`crate::billing_bills`) — be added to a euro total in a euro ledger without
/// anybody inventing a rate. Anything genuinely foreign still needs a snapshot
/// naming `base`, and gets `None` without one.
pub fn restated_open_cents(
    base: &str,
    currency: &str,
    fx: Option<&FxSnapshot>,
    cents: i64,
) -> Option<i64> {
    if currency == base {
        return Some(cents);
    }
    let fx = fx?;
    if fx.base_currency != base {
        return None;
    }
    convert_cents(cents, fx.rate_micro)
}

/// The rate of `quote` against `base` when both are published against a third
/// currency (the euro, in the reference-rate table): `1 base = ? quote`.
///
/// ```text
/// cross_micro = round(quote_micro × 1_000_000 / base_micro)
/// ```
///
/// One rounding, in `i128`, and the result is what gets **snapshotted** on the
/// document — so the number an auditor recomputes from is the number that was
/// applied, not a pair of rates they have to re-cross themselves.
///
/// Returns `None` when either input cannot be a rate, or when the cross falls
/// outside [`RATE_MICRO_MAX`] (a pair of extreme quotes; it cannot arise from
/// stored rates, which are individually bounded, but the function is total).
pub fn cross_rate_micro(quote_micro: i64, base_micro: i64) -> Option<i64> {
    if quote_micro <= 0 || base_micro <= 0 {
        return None;
    }
    if quote_micro == base_micro {
        return Some(IDENTITY_RATE_MICRO);
    }
    let exact = i128::from(quote_micro) * i128::from(RATE_SCALE);
    let crossed = div_round_half_away(exact, i128::from(base_micro));
    let crossed = i64::try_from(crossed).ok()?;
    (1..=RATE_MICRO_MAX).contains(&crossed).then_some(crossed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::billing_totals::{LineFigures, totals};
    use time::{Date, Month};

    fn day(year: i32, month: Month, day: u8) -> Date {
        Date::from_calendar_date(year, month, day).unwrap_or_else(|e| panic!("{e}"))
    }

    fn message(result: Result<i64>) -> String {
        match result {
            Err(StoreError::Validation(msg)) => msg,
            other => panic!("expected Validation, got {other:?}"),
        }
    }

    #[test]
    fn a_published_rate_reads_as_micro_units() {
        // The three shapes the reference rates actually come in: a near-unity
        // pair to four decimals, the yen to two, and a whole number.
        assert_eq!(parse_rate("1.1626").unwrap_or_default(), 1_162_600);
        assert_eq!(parse_rate("163.42").unwrap_or_default(), 163_420_000);
        assert_eq!(parse_rate("7").unwrap_or_default(), 7_000_000);
        assert_eq!(parse_rate(" 0.912345 ").unwrap_or_default(), 912_345);
        assert_eq!(
            parse_rate("1.000000").unwrap_or_default(),
            IDENTITY_RATE_MICRO
        );
        // Six decimals is the scale; five significant figures is what is
        // published, so nothing real is lost.
        assert_eq!(parse_rate("0.000001").unwrap_or_default(), 1);
    }

    #[test]
    fn a_rate_that_is_not_a_plain_decimal_is_refused_never_guessed_at() {
        for bad in [
            "",
            "   ",
            "1,1626", // a comma could be either separator
            "1.",     // a typo, not "one"
            ".5",     // a typo, not "a half"
            "-1.16",
            "1e3",
            "1.1626001", // seven decimals: refused, not truncated
            "0",
            "0.0",
            "abc",
            "1 626",
            "١.٥",
        ] {
            assert!(
                matches!(parse_rate(bad), Err(StoreError::Validation(_))),
                "expected refusal: {bad:?}"
            );
        }
        assert!(message(parse_rate("1,1626")).contains("6 decimal places"));
    }

    #[test]
    fn a_rate_outside_the_bounds_is_refused() {
        assert!(rate_micro(1).is_ok());
        assert!(rate_micro(RATE_MICRO_MAX).is_ok());
        for bad in [0, -1, RATE_MICRO_MAX + 1, i64::MIN, i64::MAX] {
            assert!(
                matches!(rate_micro(bad), Err(StoreError::Validation(_))),
                "expected refusal: {bad}"
            );
        }
        // The parser refuses the same values, in its own words.
        assert!(parse_rate("1000000000.000001").is_err());
    }

    #[test]
    fn a_rate_prints_as_the_decimal_it_was_published_as() {
        assert_eq!(format_rate(1_162_600), "1.1626");
        assert_eq!(format_rate(163_420_000), "163.42");
        assert_eq!(format_rate(IDENTITY_RATE_MICRO), "1.0");
        assert_eq!(format_rate(912_345), "0.912345");
        assert_eq!(format_rate(1), "0.000001");
        assert_eq!(format_rate(0), "0.0");
        // Total for any i64, including one no rate can hold.
        assert_eq!(format_rate(i64::MIN), "-9223372036854.775808");
    }

    #[test]
    fn a_published_rate_round_trips_through_both_forms() {
        for text in ["1.1626", "163.42", "7.0", "0.912345", "1.5"] {
            let micro = parse_rate(text).unwrap_or_else(|e| panic!("{text}: {e:?}"));
            assert_eq!(format_rate(micro), text, "{text}");
        }
    }

    #[test]
    fn an_amount_crosses_by_dividing_the_base_per_unit_rate() {
        // 1 EUR = 1.1626 USD, so USD 116.26 is EUR 100.00 exactly.
        assert_eq!(convert_cents(11_626, 1_162_600), Some(10_000));
        // And a figure that does not divide evenly rounds once, half away from
        // zero: 100.00 USD / 1.1626 = 86.0141… → 86.01.
        assert_eq!(convert_cents(10_000, 1_162_600), Some(8_601));
        // The yen, quoted the other way round in magnitude: ¥16 342 → €100.
        assert_eq!(convert_cents(1_634_200, 163_420_000), Some(10_000));
    }

    #[test]
    fn the_identity_rate_leaves_an_amount_untouched() {
        for cents in [0, 1, -1, 99_999, i64::MAX, i64::MIN] {
            assert_eq!(convert_cents(cents, IDENTITY_RATE_MICRO), Some(cents));
        }
    }

    #[test]
    fn a_rate_that_cannot_divide_converts_nothing() {
        for bad in [0, -1, i64::MIN] {
            assert_eq!(convert_cents(10_000, bad), None, "{bad}");
            assert_eq!(convert_totals(&totals(&[]), bad), None, "{bad}");
        }
    }

    #[test]
    fn crossing_a_negative_amount_mirrors_crossing_its_positive() {
        // The property that keeps a credit note the exact mirror of its
        // original after conversion, so a corrected period sums to zero in the
        // base currency too — the same reason `billing_totals` rounds half away
        // from zero rather than half up.
        let mut seed = 0x9E37_79B9_7F4A_7C15_u64;
        for _ in 0..2_000 {
            seed ^= seed >> 12;
            seed ^= seed << 25;
            seed ^= seed >> 27;
            let cents = (seed.wrapping_mul(0x2545_F491_4F6C_DD1D) % 20_000_000) as i64 - 10_000_000;
            let rate = 1 + (seed % 500_000_000) as i64;
            assert_eq!(
                convert_cents(cents, rate).map(i64::saturating_neg),
                convert_cents(-cents, rate),
                "{cents} at {rate}"
            );
        }
    }

    #[test]
    fn a_document_is_crossed_row_by_row_and_its_rows_still_add_up() {
        // Two rates on one document: net 500.00 at 21 % (VAT 105.00) and
        // 250.00 at 9 % (VAT 22.50), in USD at 1 EUR = 1.1626 USD.
        let document = totals(&[
            LineFigures {
                qty_milli: 1_000,
                unit_price_cents: 50_000,
                vat_rate_bp: 2100,
            },
            LineFigures {
                qty_milli: 1_000,
                unit_price_cents: 25_000,
                vat_rate_bp: 900,
            },
        ]);
        let base = convert_totals(&document, 1_162_600).unwrap_or_else(|| unreachable!());
        // Hand-computed, outside the code under test:
        //   9 %: net 25 000 / 1.1626 = 21 503.52… → 21 504; VAT 2 250 → 1 935.31… → 1 935
        //  21 %: net 50 000 / 1.1626 = 43 007.05… → 43 007; VAT 10 500 → 9 031.48… → 9 031
        assert_eq!(
            base.vat_by_rate,
            vec![
                VatSubtotal {
                    rate_bp: 900,
                    net_cents: 21_504,
                    vat_cents: 1_935,
                },
                VatSubtotal {
                    rate_bp: 2100,
                    net_cents: 43_007,
                    vat_cents: 9_031,
                },
            ]
        );
        assert_eq!(base.net_cents, 21_504 + 43_007);
        assert_eq!(base.vat_cents, 1_935 + 9_031);
        assert_eq!(base.gross_cents, base.net_cents + base.vat_cents);
        // The rows are exactly the totals — the property a return is filed on.
        assert_eq!(
            base.net_cents,
            base.vat_by_rate.iter().map(|r| r.net_cents).sum::<i64>()
        );
        assert_eq!(
            base.vat_cents,
            base.vat_by_rate.iter().map(|r| r.vat_cents).sum::<i64>()
        );
    }

    #[test]
    fn a_document_in_its_own_base_currency_crosses_to_itself() {
        let document = totals(&[LineFigures {
            qty_milli: 3_000,
            unit_price_cents: 333,
            vat_rate_bp: 2100,
        }]);
        assert_eq!(
            convert_totals(&document, IDENTITY_RATE_MICRO).as_ref(),
            Some(&document)
        );
        // And an empty document crosses to an empty one, not to a refusal.
        assert_eq!(
            convert_totals(&totals(&[]), 1_162_600),
            Some(Totals::default())
        );
    }

    #[test]
    fn a_cross_rate_is_computed_once_and_snapshotted() {
        // 1 EUR = 1.1626 USD and 1 EUR = 4.2755 PLN, so for a PLN-based issuer
        // 1 PLN = 1.1626 / 4.2755 = 0.2719213… USD.
        assert_eq!(cross_rate_micro(1_162_600, 4_275_500), Some(271_921));
        // A currency against itself is the identity, exactly.
        assert_eq!(
            cross_rate_micro(1_162_600, 1_162_600),
            Some(IDENTITY_RATE_MICRO)
        );
        // And the euro side of a euro-quoted table is the identity too.
        assert_eq!(
            cross_rate_micro(163_420_000, IDENTITY_RATE_MICRO),
            Some(163_420_000)
        );
        for (quote, base) in [(0, 1_000_000), (1_000_000, 0), (-1, -1), (i64::MIN, 1)] {
            assert_eq!(cross_rate_micro(quote, base), None, "{quote}/{base}");
        }
        // A cross that would leave the bounds is refused rather than stored.
        assert_eq!(cross_rate_micro(RATE_MICRO_MAX, 1), None);
    }

    #[test]
    fn an_open_amount_is_crossed_only_when_there_is_something_to_cross() {
        let on = day(2026, Month::August, 7);
        // Already in the books' currency: itself, snapshot or no snapshot.
        assert_eq!(
            restated_open_cents("EUR", "EUR", None, 121_000),
            Some(121_000)
        );
        assert_eq!(
            restated_open_cents("EUR", "EUR", Some(&FxSnapshot::identity("EUR", on)), -2_500),
            Some(-2_500)
        );
        // Foreign, with the rate frozen on the document: 1 EUR = 1.10 USD.
        let usd = FxSnapshot {
            base_currency: "EUR".to_owned(),
            rate_micro: 1_100_000,
            rate_date: on,
        };
        assert_eq!(
            restated_open_cents("EUR", "USD", Some(&usd), 100_000),
            Some(90_909)
        );
        // Foreign with no snapshot, or one taken against books the tenant no
        // longer keeps: unconvertible, never converted at a guessed rate.
        assert_eq!(restated_open_cents("EUR", "USD", None, 100_000), None);
        assert_eq!(
            restated_open_cents(
                "EUR",
                "USD",
                Some(&FxSnapshot::identity("USD", on)),
                100_000
            ),
            None
        );
    }

    #[test]
    fn the_identity_snapshot_restates_nothing() {
        let snapshot = FxSnapshot::identity("EUR", day(2026, Month::August, 7));
        assert_eq!(snapshot.rate_micro, IDENTITY_RATE_MICRO);
        assert!(
            !snapshot.restates("EUR"),
            "one figure, not the same one twice"
        );
        assert!(snapshot.restates("USD"), "a foreign document is restated");
    }
}
