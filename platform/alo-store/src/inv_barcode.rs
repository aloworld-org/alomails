//! GTIN barcodes — pure format and check-digit validation for alo Inventory
//! (ADR 0035, wave B5, `docs/design/inventory.md`, "Barcodes").
//!
//! The code on the box is the one identifier a warehouse reads with a machine
//! rather than with its eyes, and the check digit exists precisely so that a
//! mistyped or misread code is rejected at the point of entry instead of
//! discovered when the wrong item ships. This module is that door: characters
//! in, a verdict out — no store handle, no database, no network, the shape
//! [`crate::vat_id`] established in B1.03.
//!
//! Three rules, each the reason for a line of code below:
//!
//! - **Digits, kept as text.** A GTIN's leading zeros are part of it, and an
//!   integer column eats them — the classic bug that makes `0012345678905`
//!   and `12345678905` different codes on two different boxes and the same
//!   row in the database. [`canonicalize`] returns a `String`.
//! - **Four lengths, no others.** GTIN-8, -12, -13 and -14 are the standard's
//!   whole vocabulary ([`GTIN_LENGTHS`]). Accepting any string costs nothing
//!   to type and makes the scan-to-find call unreliable forever, because one
//!   bad row means a scan can match the wrong product.
//! - **Blank is valid.** Plenty of stock has no barcode at all, so empty
//!   input — including input that was only separators — is `Ok(None)` rather
//!   than an error, exactly as a B2C customer's absent VAT id is.
//!
//! Errors carry the rule, never the code. Validation messages travel into
//! logs, and a barcode is a fact about a tenant's stock.

use thiserror::Error;

/// The lengths the GTIN standard defines: GTIN-8, GTIN-12 (UPC-A), GTIN-13
/// (EAN-13) and GTIN-14 (the case/carton code).
pub const GTIN_LENGTHS: [usize; 4] = [8, 12, 13, 14];

/// Longest accepted barcode, so an over-long input is refused before any
/// arithmetic runs on it.
pub const BARCODE_MAX_CHARS: usize = 14;

/// Why a barcode was refused. Carries no part of the code itself.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum BarcodeError {
    /// The code contains something that is not a digit, after spaces and
    /// hyphens have been removed as presentation.
    #[error("a barcode may only contain digits")]
    Charset,
    /// The code is not one of the four GTIN lengths.
    #[error("a barcode must be 8, 12, 13 or 14 digits")]
    Length,
    /// The digits are right in number but their check digit disagrees with
    /// them — almost always a typo or a misread scan.
    #[error("the check digit of this barcode does not match; check for a typo")]
    Checksum,
}

/// Validates a barcode and returns its canonical form: digits only, no
/// separators, leading zeros preserved.
///
/// Blank input — including input that is only separators — is `Ok(None)`: an
/// item with no barcode is the normal case, not an error.
///
/// # Errors
/// [`BarcodeError::Charset`] for anything that is not a digit, space or
/// hyphen; [`BarcodeError::Length`] when the digit count is not one of
/// [`GTIN_LENGTHS`]; [`BarcodeError::Checksum`] when the last digit does not
/// match the rest.
///
/// # Examples
/// ```
/// use alo_store::inv_barcode::canonicalize;
/// assert_eq!(canonicalize(" 400-638 133 393 1 ").unwrap().as_deref(), Some("4006381333931"));
/// assert_eq!(canonicalize("   ").unwrap(), None);
/// ```
pub fn canonicalize(input: &str) -> Result<Option<String>, BarcodeError> {
    let mut digits = String::with_capacity(input.len());
    for ch in input.chars() {
        match ch {
            // Presentation, not content: a code is read off a box in groups.
            ' ' | '-' | '\t' | '\u{a0}' => {}
            d if d.is_ascii_digit() => {
                if digits.len() == BARCODE_MAX_CHARS {
                    return Err(BarcodeError::Length);
                }
                digits.push(d);
            }
            _ => return Err(BarcodeError::Charset),
        }
    }
    if digits.is_empty() {
        return Ok(None);
    }
    if !GTIN_LENGTHS.contains(&digits.len()) {
        return Err(BarcodeError::Length);
    }
    if !check_digit_matches(&digits) {
        return Err(BarcodeError::Checksum);
    }
    Ok(Some(digits))
}

/// Whether the final digit of `digits` is the GTIN check digit of the ones
/// before it.
///
/// The standard's rule, and it is one rule for all four lengths: weight the
/// digits **from the right**, starting at the digit left of the check digit,
/// alternately 3 and 1; the weighted sum plus the check digit must be a
/// multiple of ten.
///
/// `digits` must be ASCII digits — [`canonicalize`] is the only caller and has
/// already proved that, which is why this takes a `&str` and cannot fail.
fn check_digit_matches(digits: &str) -> bool {
    let mut sum: u32 = 0;
    // `rev()` puts the check digit first, so weight 3 starts one place later.
    for (place, byte) in digits.bytes().rev().enumerate() {
        let value = u32::from(byte - b'0');
        sum += match place {
            0 => value,
            p if p % 2 == 1 => value * 3,
            _ => value,
        };
    }
    sum.is_multiple_of(10)
}

/// The check digit that completes `body` (the code **without** its final
/// digit), or `None` when `body` is not 7, 11, 12 or 13 digits.
///
/// Useful to a caller that has a code from a label printer or a supplier feed
/// and needs the digit the standard would append; the validation path itself
/// does not use it.
///
/// # Examples
/// ```
/// use alo_store::inv_barcode::check_digit;
/// assert_eq!(check_digit("400638133393"), Some(1));
/// ```
pub fn check_digit(body: &str) -> Option<u8> {
    if !body.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    if !GTIN_LENGTHS.contains(&(body.len() + 1)) {
        return None;
    }
    let mut sum: u32 = 0;
    // The body is the code minus its check digit, so the first digit from the
    // right carries weight 3.
    for (place, byte) in body.bytes().rev().enumerate() {
        let value = u32::from(byte - b'0');
        sum += if place % 2 == 0 { value * 3 } else { value };
    }
    let digit = (10 - (sum % 10)) % 10;
    u8::try_from(digit).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Real codes off real boxes, one per GTIN length.
    const VALID: [&str; 6] = [
        "96385074",       // GTIN-8
        "012345678905",   // GTIN-12 (UPC-A), leading zero and all
        "4006381333931",  // GTIN-13 (EAN-13)
        "5901234123457",  // GTIN-13
        "00012345600012", // GTIN-14 (carton)
        "10614141000415", // GTIN-14
    ];

    #[test]
    fn real_codes_pass_at_every_length() {
        for code in VALID {
            assert_eq!(
                canonicalize(code),
                Ok(Some(code.to_owned())),
                "expected valid: {code}"
            );
        }
    }

    #[test]
    fn a_leading_zero_is_part_of_the_code() {
        // The bug this module exists to prevent: these are two different codes
        // on two different boxes, and both are valid.
        let padded = canonicalize("012345678905").unwrap_or_else(|e| panic!("rejected: {e}"));
        assert_eq!(padded.as_deref(), Some("012345678905"));
        let unpadded = canonicalize("00012345600012").unwrap_or_else(|e| panic!("rejected: {e}"));
        assert_eq!(unpadded.as_deref(), Some("00012345600012"));
        assert_ne!(padded, unpadded);
    }

    #[test]
    fn separators_are_presentation() {
        assert_eq!(
            canonicalize(" 400-638 133 393 1 "),
            Ok(Some("4006381333931".to_owned()))
        );
    }

    #[test]
    fn blank_is_not_an_error() {
        for blank in ["", "   ", "\t", " - - ", "\u{a0}"] {
            assert_eq!(canonicalize(blank), Ok(None), "expected blank: {blank:?}");
        }
    }

    #[test]
    fn a_single_wrong_digit_is_caught() {
        // Every single-digit change to a valid code must be refused — that is
        // the whole point of the check digit, and the typo it catches.
        for code in VALID {
            for position in 0..code.len() {
                for replacement in b'0'..=b'9' {
                    let mut bytes = code.as_bytes().to_vec();
                    if bytes[position] == replacement {
                        continue;
                    }
                    bytes[position] = replacement;
                    let typo = String::from_utf8(bytes).unwrap_or_else(|e| panic!("utf8: {e}"));
                    assert_eq!(
                        canonicalize(&typo),
                        Err(BarcodeError::Checksum),
                        "a typo slipped through: {typo}"
                    );
                }
            }
        }
    }

    #[test]
    fn a_wrong_length_is_refused_before_any_arithmetic() {
        for bad in ["1", "1234567", "123456789", "0123456789012345"] {
            assert_eq!(canonicalize(bad), Err(BarcodeError::Length), "{bad}");
        }
    }

    #[test]
    fn letters_are_refused() {
        for bad in ["4006381333A31", "ean 4006381333931", "4006381333931 "] {
            let verdict = canonicalize(bad);
            if bad.ends_with(' ') {
                // A trailing space is presentation; that one is fine.
                assert!(verdict.is_ok(), "{bad}");
            } else {
                assert_eq!(verdict, Err(BarcodeError::Charset), "{bad}");
            }
        }
    }

    #[test]
    fn the_error_never_carries_the_code() {
        // Validation messages travel into logs, and a barcode is a fact about
        // a tenant's stock.
        for code in ["4006381333930", "12345", "40063813339A1"] {
            match canonicalize(code) {
                Err(refused) => assert!(
                    !refused.to_string().contains(code),
                    "the message carried the code: {refused}"
                ),
                Ok(other) => panic!("expected a refusal for {code}, got {other:?}"),
            }
        }
    }

    #[test]
    fn check_digit_completes_a_body() {
        for code in VALID {
            let (body, last) = code.split_at(code.len() - 1);
            let expected = last.parse::<u8>().unwrap_or_else(|e| panic!("digit: {e}"));
            assert_eq!(check_digit(body), Some(expected), "{code}");
        }
        // A body of the wrong length has no answer, and neither has a
        // non-numeric one.
        assert_eq!(check_digit("123"), None);
        assert_eq!(check_digit("40063813339A"), None);
    }
}
