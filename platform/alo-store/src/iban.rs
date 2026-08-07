//! IBANs — pure format and check-digit validation for alo Billing (ADR 0035,
//! wave B1, `docs/design/billing.md`).
//!
//! An IBAN is printed on every invoice a tenant issues and is what the money
//! is transferred to. A mistyped one is not discovered by anybody until a
//! payment fails to arrive, weeks later, so it is worth catching at the point
//! of entry — and unlike a VAT id, an IBAN carries its own check digits by
//! design (ISO 13616 / ISO 7064 MOD 97-10), so the check is exact rather than
//! a heuristic.
//!
//! Three rules, the same shape as [`crate::vat_id`]:
//!
//! - **No network.** This says the number is well-formed, never that the
//!   account exists — that is a bank's answer, not a validator's.
//! - **Length is per country.** ISO 13616 fixes a length for each registered
//!   country (DE 22, NL 18, FR 27 …). A country we do not have a length for
//!   passes on the generic rules alone: an unknown length is not a reason to
//!   refuse a real account, and the mod-97 check still has to hold.
//! - **The stored form is canonical**: uppercase, no spaces. Spaces are
//!   presentation — the printed document groups it in fours ([`grouped`]).

use thiserror::Error;

/// Longest IBAN the standard allows (Saint Lucia and Russia, at 33).
pub const IBAN_MAX_CHARS: usize = 34;
/// Shortest registered IBAN (Norway, at 15).
pub const IBAN_MIN_CHARS: usize = 15;

/// Why an IBAN was refused. Carries no part of the number itself — errors
/// travel into logs, and a bank account number does not (law 1).
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum IbanError {
    /// Something that is neither a letter nor a digit survived the removal of
    /// spaces, which are the only separator an IBAN is ever written with.
    #[error("IBAN may only contain letters and digits")]
    Charset,
    /// Not `CCkk` followed by at least eleven alphanumerics, within the
    /// standard's overall length bounds.
    #[error(
        "an IBAN is two letters, two digits and then {IBAN_MIN_CHARS}–{IBAN_MAX_CHARS} characters in total"
    )]
    Shape,
    /// The number is the wrong length for the country it names.
    #[error("a {0} IBAN is {1} characters long")]
    Length(&'static str, usize),
    /// Shape and length are right but the check digits are not — a typo.
    #[error("the check digits of this IBAN do not match; check for a typo")]
    Checksum,
}

/// The registered IBAN length for each country that has one, ISO 13616 as
/// published by SWIFT. A country absent from this table is validated on the
/// generic rules and its check digits only.
const LENGTHS: &[(&str, usize)] = &[
    ("AD", 24),
    ("AE", 23),
    ("AL", 28),
    ("AT", 20),
    ("AZ", 28),
    ("BA", 20),
    ("BE", 16),
    ("BG", 22),
    ("BH", 22),
    ("BR", 29),
    ("BY", 28),
    ("CH", 21),
    ("CR", 22),
    ("CY", 28),
    ("CZ", 24),
    ("DE", 22),
    ("DK", 18),
    ("DO", 28),
    ("EE", 20),
    ("EG", 29),
    ("ES", 24),
    ("FI", 18),
    ("FO", 18),
    ("FR", 27),
    ("GB", 22),
    ("GE", 22),
    ("GI", 23),
    ("GL", 18),
    ("GR", 27),
    ("GT", 28),
    ("HR", 21),
    ("HU", 28),
    ("IE", 22),
    ("IL", 23),
    ("IQ", 23),
    ("IS", 26),
    ("IT", 27),
    ("JO", 30),
    ("KW", 30),
    ("KZ", 20),
    ("LB", 28),
    ("LC", 32),
    ("LI", 21),
    ("LT", 20),
    ("LU", 20),
    ("LV", 21),
    ("LY", 25),
    ("MC", 27),
    ("MD", 24),
    ("ME", 22),
    ("MK", 19),
    ("MR", 27),
    ("MT", 31),
    ("MU", 30),
    ("NL", 18),
    ("NO", 15),
    ("PK", 24),
    ("PL", 28),
    ("PS", 29),
    ("PT", 25),
    ("QA", 29),
    ("RO", 24),
    ("RS", 22),
    ("SA", 24),
    ("SC", 31),
    ("SD", 18),
    ("SE", 24),
    ("SI", 19),
    ("SK", 24),
    ("SM", 27),
    ("ST", 25),
    ("SV", 28),
    ("TL", 23),
    ("TN", 24),
    ("TR", 26),
    ("UA", 29),
    ("VA", 22),
    ("VG", 24),
    ("XK", 20),
];

/// Removes the spaces an IBAN is written with and uppercases the rest.
///
/// Uppercasing is **ASCII-only**, deliberately. Unicode uppercasing can make a
/// string longer (`ß` → `SS`, `ﬁ` → `FI`), which would turn a seven-character
/// BIC into a valid-looking eight-character one that is not what the user
/// typed — and a BIC, unlike an IBAN, has no check digits to catch it. A
/// character outside ASCII survives this untouched and is then refused by the
/// charset rule, which is the honest answer: we do not know what the user
/// meant.
fn compact(input: &str) -> String {
    input
        .chars()
        .filter(|c| !c.is_whitespace())
        .map(|c| c.to_ascii_uppercase())
        .collect()
}

/// The ISO 7064 MOD 97-10 remainder of a compacted IBAN.
///
/// The standard moves the first four characters to the end, replaces each
/// letter by its position in the alphabet plus 9 (`A` = 10 … `Z` = 35) and
/// takes the resulting decimal number modulo 97, which must be 1. The number
/// is far too long for any integer type, so the remainder is carried digit by
/// digit — which is exactly how the standard describes doing it by hand.
///
/// Returns `None` if a character is not alphanumeric; callers check the
/// charset first, so that is a guard, not a path.
fn mod97(iban: &str) -> Option<u32> {
    let (head, tail) = iban.split_at(4);
    let mut remainder: u32 = 0;
    for c in tail.chars().chain(head.chars()) {
        let value = if c.is_ascii_digit() {
            u32::from(c as u8 - b'0')
        } else if c.is_ascii_uppercase() {
            u32::from(c as u8 - b'A') + 10
        } else {
            return None;
        };
        // Two digits at a time when the letter expanded to two: the remainder
        // stays well inside u32 either way (97 * 100 + 35 fits easily).
        remainder = if value > 9 {
            (remainder * 100 + value) % 97
        } else {
            (remainder * 10 + value) % 97
        };
    }
    Some(remainder)
}

/// Validates and canonicalises an IBAN.
///
/// Blank input — including input that is only spaces — is `Ok(None)`: a
/// tenant that has not stated a bank account has not made a mistake.
///
/// The number may be written with any spacing and in any case
/// (`de89 3704 0044 0532 0130 00`, `DE89370400440532013000`); the canonical
/// form is uppercase and unspaced, which is what SEPA messages
/// (`pain.001`, B2.12) and EN 16931 both want.
///
/// # Errors
/// [`IbanError`] naming the rule that failed; never echoing the number.
pub fn canonicalize(input: &str) -> Result<Option<String>, IbanError> {
    let compact = compact(input);
    if compact.is_empty() {
        return Ok(None);
    }
    if !compact.chars().all(|c| c.is_ascii_alphanumeric()) {
        return Err(IbanError::Charset);
    }
    let bytes = compact.as_bytes();
    let shaped = (IBAN_MIN_CHARS..=IBAN_MAX_CHARS).contains(&compact.len())
        && bytes[0].is_ascii_uppercase()
        && bytes[1].is_ascii_uppercase()
        && bytes[2].is_ascii_digit()
        && bytes[3].is_ascii_digit();
    if !shaped {
        return Err(IbanError::Shape);
    }
    let country = &compact[..2];
    if let Some(&(code, len)) = LENGTHS.iter().find(|(code, _)| *code == country)
        && compact.len() != len
    {
        return Err(IbanError::Length(code, len));
    }
    if mod97(&compact) != Some(1) {
        return Err(IbanError::Checksum);
    }
    Ok(Some(compact))
}

/// Groups a canonical IBAN in fours, the way one is printed on a document and
/// read out loud. Presentation only — never the stored form.
pub fn grouped(iban: &str) -> String {
    let mut out = String::with_capacity(iban.len() + iban.len() / 4);
    for (i, c) in iban.chars().enumerate() {
        if i > 0 && i.is_multiple_of(4) {
            out.push(' ');
        }
        out.push(c);
    }
    out
}

/// Longest BIC (ISO 9362): 8 characters, or 11 with a branch code.
pub const BIC_MAX_CHARS: usize = 11;

/// Why a BIC was refused.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum BicError {
    /// Not the ISO 9362 shape: four letters (institution), two letters
    /// (country), two alphanumerics (location), optionally three more
    /// (branch).
    #[error("a BIC is 8 or 11 letters and digits (AAAABBCCXXX)")]
    Shape,
}

/// Validates and canonicalises a BIC/SWIFT code. Blank is `Ok(None)`.
///
/// Shape only, and deliberately so: the institution and location parts are a
/// registry, not an algorithm, and a BIC we refuse is a payment a tenant
/// cannot print. The country part is checked as letters, not against a list,
/// for the reason [`crate::billing_field`] gives for country codes generally.
///
/// # Errors
/// [`BicError::Shape`] when the shape does not hold; never echoing the value.
pub fn canonicalize_bic(input: &str) -> Result<Option<String>, BicError> {
    let compact = compact(input);
    if compact.is_empty() {
        return Ok(None);
    }
    let bytes = compact.as_bytes();
    let shaped = (compact.len() == 8 || compact.len() == BIC_MAX_CHARS)
        && bytes[..4].iter().all(u8::is_ascii_alphabetic)
        && bytes[4..6].iter().all(u8::is_ascii_alphabetic)
        && bytes[6..].iter().all(u8::is_ascii_alphanumeric);
    if !shaped {
        return Err(BicError::Shape);
    }
    Ok(Some(compact))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Real, published specimen IBANs — the ones every bank's own examples
    /// use — so the mod-97 implementation is checked against numbers that
    /// actually validate rather than against itself.
    const VALID: &[&str] = &[
        "DE89370400440532013000",
        "NL91ABNA0417164300",
        "FR1420041010050500013M02606",
        "BE68539007547034",
        "GB82WEST12345698765432",
        "PL61109010140000071219812874",
        "IT60X0542811101000000123456",
        "ES9121000418450200051332",
        "NO9386011117947",
        "MT84MALT011000012345MTLCAST001S",
    ];

    #[test]
    fn published_specimens_all_validate() {
        for iban in VALID {
            assert_eq!(
                canonicalize(iban),
                Ok(Some((*iban).to_owned())),
                "expected valid: {iban}"
            );
        }
    }

    #[test]
    fn spacing_and_case_are_presentation() {
        assert_eq!(
            canonicalize("de89 3704 0044 0532 0130 00"),
            Ok(Some("DE89370400440532013000".to_owned()))
        );
        assert_eq!(
            canonicalize("\tNL91 abna 0417 1643 00 \n"),
            Ok(Some("NL91ABNA0417164300".to_owned()))
        );
    }

    #[test]
    fn blank_is_not_an_error() {
        for blank in ["", "   ", "\t\n"] {
            assert_eq!(canonicalize(blank), Ok(None));
        }
    }

    #[test]
    fn a_single_transposition_is_caught() {
        // Swapping two adjacent digits is the commonest typo there is, and
        // the mod-97 check exists precisely to catch it: the two below are
        // the German specimen with `30` transposed and with one wrong check
        // digit, and neither is distinguishable from the real thing by eye.
        assert_eq!(
            canonicalize("DE89370400440532010300"),
            Err(IbanError::Checksum)
        );
        assert_eq!(
            canonicalize("DE90370400440532013000"),
            Err(IbanError::Checksum)
        );
    }

    #[test]
    fn country_length_is_enforced_before_the_checksum() {
        // One digit short for Germany: the length is the useful complaint,
        // not "the check digits do not match".
        assert_eq!(
            canonicalize("DE8937040044053201300"),
            Err(IbanError::Length("DE", 22))
        );
        assert_eq!(
            canonicalize("NL91ABNA04171643001"),
            Err(IbanError::Length("NL", 18))
        );
    }

    #[test]
    fn shape_is_two_letters_then_two_digits() {
        for bad in [
            "1E89370400440532013000",              // digit where the country goes
            "DEX9370400440532013000",              // letter where a check digit goes
            "DE8",                                 // far too short
            "NO938601111794",                      // 14: below the standard's minimum
            "DE893704004405320130001234567890123", // 35: above the maximum
        ] {
            assert_eq!(
                canonicalize(bad),
                Err(IbanError::Shape),
                "expected Shape: {bad}"
            );
        }
    }

    #[test]
    fn separators_other_than_spaces_are_refused() {
        // Hyphens and dots are tolerated on a VAT id because they are printed
        // that way; an IBAN is only ever written in spaced groups, so
        // anything else is a paste from the wrong field.
        assert_eq!(
            canonicalize("DE89-3704-0044-0532-0130-00"),
            Err(IbanError::Charset)
        );
        assert_eq!(
            canonicalize("DE89.3704.0044.0532.0130.00"),
            Err(IbanError::Charset)
        );
    }

    #[test]
    fn an_unknown_country_passes_on_the_checksum_alone() {
        // ZZ is not a registered IBAN country; the number below has correct
        // mod-97 check digits, so it is accepted rather than blocking a
        // tenant whose country joined the registry after this table.
        let candidate = "ZZ00ABCD1234567890";
        let fixed = with_valid_check_digits(candidate);
        assert_eq!(canonicalize(&fixed), Ok(Some(fixed.clone())));
        // …and the checksum is still the gate for it.
        assert!(matches!(
            canonicalize(&flip_last_digit(&fixed)),
            Err(IbanError::Checksum)
        ));
    }

    #[test]
    fn grouping_is_presentation_only() {
        assert_eq!(
            grouped("DE89370400440532013000"),
            "DE89 3704 0044 0532 0130 00"
        );
        assert_eq!(grouped(""), "");
        assert_eq!(grouped("AB"), "AB");
    }

    #[test]
    fn uppercasing_never_lengthens_what_was_typed() {
        // Unicode uppercasing expands (`ß` → `SS`, U+FB01 `ﬁ` → `FI`). If we
        // used it, a seven-character BIC would become a plausible-looking
        // eight-character one — and a BIC has no check digits to catch it.
        assert_eq!(canonicalize_bic("deutdeﬁ"), Err(BicError::Shape));
        assert_eq!(canonicalize_bic("deutdeß"), Err(BicError::Shape));
        // The same on an IBAN, where the length rule sees the true length.
        assert_eq!(
            canonicalize("DE8937040044053201300ß"),
            Err(IbanError::Charset)
        );
    }

    #[test]
    fn bic_takes_both_lengths_and_refuses_the_rest() {
        assert_eq!(
            canonicalize_bic("deutdeff"),
            Ok(Some("DEUTDEFF".to_owned()))
        );
        assert_eq!(
            canonicalize_bic("DEUT DE FF 500"),
            Ok(Some("DEUTDEFF500".to_owned()))
        );
        assert_eq!(canonicalize_bic(""), Ok(None));
        for bad in [
            "DEUTDEF",
            "DEUTDEFF5",
            "DEUTDEFF5000",
            "DEU1DEFF",
            "DEUTD1FF",
        ] {
            assert_eq!(
                canonicalize_bic(bad),
                Err(BicError::Shape),
                "expected Shape: {bad}"
            );
        }
    }

    /// Rewrites positions 2–3 so the number carries valid check digits —
    /// the same computation a bank does when it issues one.
    fn with_valid_check_digits(candidate: &str) -> String {
        let zeroed = format!("{}00{}", &candidate[..2], &candidate[4..]);
        let remainder = mod97(&zeroed).unwrap_or(0);
        let check = 98 - remainder;
        format!("{}{:02}{}", &candidate[..2], check, &candidate[4..])
    }

    fn flip_last_digit(iban: &str) -> String {
        let mut out = iban.to_owned();
        let last = out.pop().unwrap_or('0');
        let next = if last == '9' { '8' } else { '9' };
        out.push(next);
        out
    }
}
