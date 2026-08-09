//! How Europe writes an amount, read as integer cents — the grammar, with no
//! opinion about what the number is for.
//!
//! Two callers need the same reading and must not disagree about it: the CRM
//! lead import ([`crate::crm_lead_import`], where the amount is a column a
//! salesperson typed) and the receipt extractor ([`crate::fin_receipt`], where
//! it is a token on a till roll). A second implementation of "is `1.234,56` a
//! thousand or one and a bit" is a bug waiting for the day the two answers
//! differ, so there is one.
//!
//! The grammar, and why each shape is unambiguous:
//!
//! - `1234`, `€ 1 234`, `1 234,00 €` — no decimal separator, or a plain one.
//!   Spaces (including the non-breaking and thin ones a spreadsheet writes),
//!   the Swiss apostrophe, and the currency symbols a European price carries
//!   are stripped before anything else happens.
//! - `1234.56` and `1234,56` — a single separator with one or two digits after
//!   it is a decimal separator, in either country's convention.
//! - `1.234.567` and `1 234 567` — a repeated separator groups thousands, and
//!   its groups must be three digits.
//! - `1.234,56` and `1,234.56` — both present: the **last** one is the decimal
//!   separator and the other is grouping, which holds in every locale that
//!   uses either.
//!
//! And the one shape it refuses rather than guesses: `1.234`, a single
//! separator before exactly three digits. It is 1234 in Berlin and 1.23 in
//! London, and money is never guessed (CLAUDE.md law 2).
//!
//! Errors are a **reason**, not a sentence: the two callers phrase the refusal
//! in their own words ("the value column…", "this token is not an amount"),
//! because a message that names a CSV column is wrong on a receipt. Nothing
//! here allocates a string, and nothing here knows a currency.

/// Why a piece of text is not an amount. A reason, not a message — the caller
/// writes the sentence its user will read.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AmountText {
    /// Nothing but separators and symbols — no digits at all. Whether "no
    /// amount" means zero or means missing is the caller's question.
    Empty,
    /// A leading minus. Signs are the caller's business too: a deal value must
    /// not be negative, a bank line very much may be.
    Negative,
    /// `1.234` — one separator, three digits after it. A thousand or one and a
    /// bit, and the difference is a factor of a thousand.
    Ambiguous,
    /// Thousands separators that do not group in threes (`1.23.456`).
    Grouping,
    /// Not a number: other characters, several separators of one kind mixed
    /// with the other, more than two decimal digits.
    NotANumber,
    /// A number so large the cents do not fit in an `i64`.
    TooLarge,
}

/// The characters that are decoration around a number rather than part of it.
const STRIPPED: &[char] = &[
    ' ', '\u{a0}',   // no-break space
    '\u{202f}', // narrow no-break space
    '\u{2009}', // thin space
    '€', '$', '£', '\'', // the Swiss thousands separator
];

/// Reads `raw` as integer cents, or says why it is not an amount.
///
/// See the module header for the shapes accepted and the one refused. The
/// result is exact: no floating point is involved at any point, which is the
/// reason this function exists rather than a `parse::<f64>()`.
///
/// # Errors
///
/// [`AmountText`], which is a reason rather than a message.
/// The number without the decoration a person or a spreadsheet put around it —
/// currency symbols, every width of space, the Swiss thousands apostrophe.
///
/// Exposed because a caller that must rewrite an amount before reading it (the
/// bank CSV wizard, [`crate::bank_csv`], which is told which separator is the
/// decimal one) has to strip exactly what [`parse_amount_cents`] strips. Two
/// lists would drift, and the drift would be a currency symbol read as a third
/// decimal.
#[must_use]
pub fn strip_decoration(raw: &str) -> String {
    raw.chars().filter(|c| !STRIPPED.contains(c)).collect()
}

pub fn parse_amount_cents(raw: &str) -> Result<i64, AmountText> {
    let cleaned = strip_decoration(raw);
    if cleaned.is_empty() {
        return Err(AmountText::Empty);
    }
    if cleaned.starts_with('-') {
        return Err(AmountText::Negative);
    }
    if !cleaned
        .chars()
        .all(|c| c.is_ascii_digit() || c == '.' || c == ',')
    {
        return Err(AmountText::NotANumber);
    }
    if !cleaned.chars().any(|c| c.is_ascii_digit()) {
        return Err(AmountText::Empty);
    }
    let dots = cleaned.matches('.').count();
    let commas = cleaned.matches(',').count();
    let (whole, frac) = match (dots, commas) {
        (0, 0) => (cleaned.clone(), String::new()),
        // One kind of separator, more than once: it groups thousands.
        (_, 0) if dots > 1 => (check_groups(&cleaned, '.')?, String::new()),
        (0, _) if commas > 1 => (check_groups(&cleaned, ',')?, String::new()),
        // One kind of separator, once: decimal, or the one ambiguous shape.
        (1, 0) => split_single(&cleaned, '.')?,
        (0, 1) => split_single(&cleaned, ',')?,
        _ => {
            // Both are present: the later one is the decimal separator, which
            // holds in every locale that uses either.
            let decimal = if cleaned.rfind('.') > cleaned.rfind(',') {
                '.'
            } else {
                ','
            };
            let grouping = if decimal == '.' { ',' } else { '.' };
            let (int, frac) = cleaned.rsplit_once(decimal).ok_or(AmountText::NotANumber)?;
            if frac.contains(grouping) || !(1..=2).contains(&frac.len()) {
                return Err(AmountText::NotANumber);
            }
            (check_groups(int, grouping)?, frac.to_owned())
        }
    };
    let units: i64 = whole.parse().map_err(|_| AmountText::NotANumber)?;
    // One digit after the separator is tenths, two is hundredths.
    let cents: i64 = match frac.len() {
        0 => 0,
        1 => frac.parse::<i64>().map_err(|_| AmountText::NotANumber)? * 10,
        _ => frac.parse().map_err(|_| AmountText::NotANumber)?,
    };
    units
        .checked_mul(100)
        .and_then(|units| units.checked_add(cents))
        .ok_or(AmountText::TooLarge)
}

/// A number with exactly one separator: decimal when one or two digits follow,
/// **ambiguous** when three do, not a number otherwise.
fn split_single(value: &str, separator: char) -> Result<(String, String), AmountText> {
    let Some((int, frac)) = value.split_once(separator) else {
        return Ok((value.to_owned(), String::new()));
    };
    if int.is_empty() {
        // `,50` — a receipt writes this for fifty cents, but so does a broken
        // OCR of `1,50`. The caller sees a refusal, not a guess.
        return Err(AmountText::NotANumber);
    }
    match frac.len() {
        1 | 2 => Ok((int.to_owned(), frac.to_owned())),
        3 => Err(AmountText::Ambiguous),
        _ => Err(AmountText::NotANumber),
    }
}

/// Validates thousands groups and returns the number without them.
fn check_groups(value: &str, separator: char) -> Result<String, AmountText> {
    let parts: Vec<&str> = value.split(separator).collect();
    let Some((first, rest)) = parts.split_first() else {
        return Err(AmountText::Grouping);
    };
    if first.is_empty() || first.len() > 3 {
        return Err(AmountText::Grouping);
    }
    if rest.iter().any(|group| group.len() != 3) {
        return Err(AmountText::Grouping);
    }
    Ok(parts.concat())
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    fn cents(raw: &str) -> i64 {
        parse_amount_cents(raw).unwrap_or_else(|e| panic!("{raw:?} refused: {e:?}"))
    }

    fn refused(raw: &str) -> AmountText {
        match parse_amount_cents(raw) {
            Err(reason) => reason,
            Ok(value) => panic!("{raw:?} read as {value} cents"),
        }
    }

    #[test]
    fn every_way_europe_writes_an_amount_reads_as_the_same_cents() {
        assert_eq!(cents("1234"), 123_400);
        assert_eq!(cents("1234.56"), 123_456);
        assert_eq!(cents("1234,56"), 123_456);
        assert_eq!(cents("1.234,56"), 123_456);
        assert_eq!(cents("1,234.56"), 123_456);
        assert_eq!(cents("1.234.567"), 123_456_700);
        assert_eq!(cents("1 234 567"), 123_456_700);
        assert_eq!(cents("1'234.50"), 123_450, "the Swiss apostrophe");
        assert_eq!(cents("€ 1 234,50"), 123_450);
        assert_eq!(cents("1234,5"), 123_450, "one decimal digit is tenths");
        assert_eq!(cents("0"), 0);
    }

    #[test]
    fn the_one_ambiguous_shape_is_refused_rather_than_guessed() {
        assert_eq!(refused("1.234"), AmountText::Ambiguous);
        assert_eq!(refused("1,234"), AmountText::Ambiguous);
        // Grouped, so no longer ambiguous.
        assert_eq!(cents("1.234.000"), 123_400_000);
    }

    #[test]
    fn a_reason_is_given_for_each_way_a_token_is_not_an_amount() {
        assert_eq!(refused(""), AmountText::Empty);
        assert_eq!(refused("€"), AmountText::Empty);
        assert_eq!(refused("-5"), AmountText::Negative);
        assert_eq!(refused("abc"), AmountText::NotANumber);
        assert_eq!(refused("12abc"), AmountText::NotANumber);
        assert_eq!(refused("1234,5678"), AmountText::NotANumber);
        assert_eq!(refused("1.23.456"), AmountText::Grouping);
        assert_eq!(refused("12.3.45"), AmountText::Grouping);
        assert_eq!(refused(",50"), AmountText::NotANumber);
    }

    #[test]
    fn an_amount_too_large_for_cents_says_so_rather_than_wrapping() {
        // Parses as an `i64` and then overflows on the hundred.
        assert_eq!(refused("1000000000000000000"), AmountText::TooLarge);
        // More digits than an `i64` holds at all is simply not a number.
        assert_eq!(refused(&"9".repeat(19)), AmountText::NotANumber);
        assert_eq!(cents("92233720368547758"), 9_223_372_036_854_775_800);
    }
}
