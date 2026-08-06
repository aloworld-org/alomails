//! EU VAT identification numbers — pure format and checksum validation for
//! alo Billing (ADR 0035, wave B1, `docs/design/billing.md`).
//!
//! A VAT id is what makes an invoice a VAT document: it decides reverse
//! charge, it is a mandatory EN 16931 field, and a mistyped one is only ever
//! discovered by the customer's accountant. This module catches the typo at
//! the door — offline, with no VIES call — by checking the **shape** the
//! member state publishes and, where the algorithm is published and
//! unambiguous, the **check digit**.
//!
//! Three deliberate rules:
//!
//! - **No network.** This is format validation, not existence validation.
//!   Confirming that a number is actually registered is a VIES lookup, which
//!   is a later, explicitly-triggered feature — never a hidden call on a
//!   create request (and never something the build loop performs).
//! - **A missing checksum is better than a wrong one.** Where a member state
//!   does not publish the algorithm (or publishes several), the id passes on
//!   shape alone. A rejected valid id blocks a real customer from being
//!   invoiced; an accepted typo is caught downstream. Silence beats guessing.
//! - **The stored form is canonical**: uppercase, no separators, and carrying
//!   its two-letter prefix (`DE811907980`), which is the form EN 16931 and
//!   every e-invoicing schema want.
//!
//! Empty is always valid: B2C customers have no VAT id ([`canonicalize`]
//! returns `Ok(None)`).

use thiserror::Error;

/// Longest VAT id we accept. The longest EU form is 14 characters (2-letter
/// prefix + 12); the margin leaves room for non-EU forms without letting an
/// essay through.
pub const VAT_ID_MAX_CHARS: usize = 20;

/// Why a VAT id was refused. Carries no part of the id itself — errors travel
/// into logs, and customer data does not.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum VatIdError {
    /// The id contains something that is neither a letter nor a digit (after
    /// spaces, dots and hyphens have been removed as presentation).
    #[error("VAT id may only contain letters and digits")]
    Charset,
    /// The id is longer than [`VAT_ID_MAX_CHARS`].
    #[error("VAT id must be at most {VAT_ID_MAX_CHARS} characters")]
    TooLong,
    /// The id does not have the shape the member state publishes. Carries the
    /// country prefix and a human description of the expected shape.
    #[error("a {0} VAT id is {1}")]
    Shape(&'static str, &'static str),
    /// The shape is right but the check digit is not — almost always a typo.
    #[error("the check digit of this {0} VAT id does not match; check for a typo")]
    Checksum(&'static str),
}

/// Validates and canonicalises a VAT identification number for a customer in
/// `country` (ISO 3166-1 alpha-2, any case).
///
/// Blank input — including input that is only separators — is `Ok(None)`: a
/// B2C customer has no VAT id and that is not an error.
///
/// The id may be written with or without its country prefix and with any of
/// the usual separators (`DE 811.907-980`, `811907980`, `de811907980` all
/// canonicalise to `DE811907980`). A **foreign** registration is accepted as
/// written when it is valid for the country it names — a French company
/// registered for VAT in Germany really does invoice under a `DE` number.
///
/// # Errors
/// [`VatIdError`] naming the rule that failed; never echoing the id.
pub fn canonicalize(input: &str, country: &str) -> Result<Option<String>, VatIdError> {
    let compact = compact(input);
    if compact.is_empty() {
        return Ok(None);
    }
    if compact.chars().count() > VAT_ID_MAX_CHARS {
        return Err(VatIdError::TooLong);
    }
    if !compact.chars().all(|c| c.is_ascii_alphanumeric()) {
        return Err(VatIdError::Charset);
    }

    // The prefix the id writes for itself, when it is one we have rules for.
    let stated = stated_prefix(&compact);
    if let Some((rules, body)) = stated
        && check(rules, body).is_ok()
    {
        return Ok(Some(compact));
    }

    let home = home_prefix(country);
    let Some(rules) = rules_for(home) else {
        // A country we publish no rules for (non-EU). Store what was typed,
        // canonicalised — refusing it would be inventing a rule.
        return match stated {
            Some((rules, body)) => check(rules, body).map(|()| Some(compact)),
            None => Ok(Some(compact)),
        };
    };

    let body = compact.strip_prefix(home).unwrap_or(compact.as_str());
    match check(rules, body) {
        Ok(()) => Ok(Some(format!("{home}{body}"))),
        // The id names a country of its own: report *that* country's rule,
        // which is the one the person typing was aiming at.
        Err(home_error) => Err(stated
            .and_then(|(rules, body)| check(rules, body).err())
            .unwrap_or(home_error)),
    }
}

/// Strips presentation — whitespace, dots, hyphens — and uppercases. Applied
/// before every other rule so `DE 811.907-980` and `de811907980` are one id.
fn compact(input: &str) -> String {
    input
        .chars()
        .filter(|c| !c.is_whitespace() && *c != '.' && *c != '-')
        .collect::<String>()
        .to_uppercase()
}

/// The VAT prefix a country issues under. Identical to the ISO country code
/// everywhere except Greece, which is `GR` as a country and `EL` as a VAT
/// prefix.
fn home_prefix(country: &str) -> &'static str {
    let country = country.trim().to_ascii_uppercase();
    if country == "GR" {
        return "EL";
    }
    RULES
        .iter()
        .find(|r| r.prefix == country)
        .map_or("", |r| r.prefix)
}

/// The rules the id names by its own first two characters, with the remaining
/// body — `None` when it does not start with a prefix we know.
fn stated_prefix(compact: &str) -> Option<(&'static CountryRules, &str)> {
    let head = compact.get(..2)?;
    if !head.bytes().all(|b| b.is_ascii_alphabetic()) {
        return None;
    }
    let rules = rules_for(head)?;
    Some((rules, &compact[2..]))
}

fn rules_for(prefix: &str) -> Option<&'static CountryRules> {
    RULES.iter().find(|r| r.prefix == prefix)
}

/// Shape first, then check digit if the member state publishes one.
fn check(rules: &CountryRules, body: &str) -> Result<(), VatIdError> {
    if !(rules.shape)(body) {
        return Err(VatIdError::Shape(rules.prefix, rules.shape_description));
    }
    if let Some(checksum) = rules.checksum
        && !checksum(body)
    {
        return Err(VatIdError::Checksum(rules.prefix));
    }
    Ok(())
}

/// One member state's rules: the published shape of the body after the
/// two-letter prefix, and its check digit when the algorithm is published and
/// unambiguous (`None` = shape only, deliberately — see the module docs).
struct CountryRules {
    prefix: &'static str,
    /// How the shape reads in an error message ("9 digits").
    shape_description: &'static str,
    shape: fn(&str) -> bool,
    checksum: Option<fn(&str) -> bool>,
}

/// The EU-27, in prefix order. Countries outside this table are accepted on
/// charset and length alone.
static RULES: &[CountryRules] = &[
    CountryRules {
        prefix: "AT",
        shape_description: "the letter U followed by 8 digits",
        shape: |b| b.len() == 9 && b.starts_with('U') && all_digits(&b[1..]),
        checksum: Some(at_checksum),
    },
    CountryRules {
        prefix: "BE",
        shape_description: "10 digits beginning with 0 or 1",
        shape: |b| b.len() == 10 && all_digits(b) && (b.starts_with('0') || b.starts_with('1')),
        checksum: Some(be_checksum),
    },
    CountryRules {
        prefix: "BG",
        shape_description: "9 or 10 digits",
        shape: |b| (b.len() == 9 || b.len() == 10) && all_digits(b),
        checksum: None,
    },
    CountryRules {
        prefix: "CY",
        shape_description: "8 digits followed by a letter",
        shape: |b| b.len() == 9 && all_digits(&b[..8]) && b.as_bytes()[8].is_ascii_alphabetic(),
        checksum: None,
    },
    CountryRules {
        prefix: "CZ",
        shape_description: "8, 9 or 10 digits",
        shape: |b| (8..=10).contains(&b.len()) && all_digits(b),
        checksum: None,
    },
    CountryRules {
        prefix: "DE",
        shape_description: "9 digits",
        shape: |b| b.len() == 9 && all_digits(b),
        checksum: Some(de_checksum),
    },
    CountryRules {
        prefix: "DK",
        shape_description: "8 digits",
        shape: |b| b.len() == 8 && all_digits(b),
        checksum: Some(dk_checksum),
    },
    CountryRules {
        prefix: "EE",
        shape_description: "9 digits",
        shape: |b| b.len() == 9 && all_digits(b),
        checksum: None,
    },
    CountryRules {
        prefix: "EL",
        shape_description: "9 digits",
        shape: |b| b.len() == 9 && all_digits(b),
        checksum: None,
    },
    CountryRules {
        prefix: "ES",
        shape_description: "9 characters with a letter first, last, or both",
        shape: |b| {
            b.len() == 9
                && b.bytes().all(|c| c.is_ascii_alphanumeric())
                && (b.as_bytes()[0].is_ascii_alphabetic() || b.as_bytes()[8].is_ascii_alphabetic())
        },
        checksum: None,
    },
    CountryRules {
        prefix: "FI",
        shape_description: "8 digits",
        shape: |b| b.len() == 8 && all_digits(b),
        checksum: Some(fi_checksum),
    },
    CountryRules {
        prefix: "FR",
        shape_description: "a 2-character key (no I or O) followed by 9 digits",
        shape: |b| {
            b.len() == 11
                && b[..2]
                    .bytes()
                    .all(|c| c.is_ascii_alphanumeric() && c != b'I' && c != b'O')
                && all_digits(&b[2..])
        },
        checksum: Some(fr_checksum),
    },
    CountryRules {
        prefix: "HR",
        shape_description: "11 digits",
        shape: |b| b.len() == 11 && all_digits(b),
        checksum: None,
    },
    CountryRules {
        prefix: "HU",
        shape_description: "8 digits",
        shape: |b| b.len() == 8 && all_digits(b),
        checksum: None,
    },
    CountryRules {
        prefix: "IE",
        shape_description: "8 or 9 characters starting with a digit",
        shape: |b| {
            (8..=9).contains(&b.len())
                && b.bytes().all(|c| c.is_ascii_alphanumeric())
                && b.as_bytes()[0].is_ascii_digit()
        },
        checksum: None,
    },
    CountryRules {
        prefix: "IT",
        shape_description: "11 digits",
        shape: |b| b.len() == 11 && all_digits(b),
        checksum: Some(it_checksum),
    },
    CountryRules {
        prefix: "LT",
        shape_description: "9 or 12 digits",
        shape: |b| (b.len() == 9 || b.len() == 12) && all_digits(b),
        checksum: None,
    },
    CountryRules {
        prefix: "LU",
        shape_description: "8 digits",
        shape: |b| b.len() == 8 && all_digits(b),
        checksum: Some(lu_checksum),
    },
    CountryRules {
        prefix: "LV",
        shape_description: "11 digits",
        shape: |b| b.len() == 11 && all_digits(b),
        checksum: None,
    },
    CountryRules {
        prefix: "MT",
        shape_description: "8 digits",
        shape: |b| b.len() == 8 && all_digits(b),
        checksum: None,
    },
    CountryRules {
        prefix: "NL",
        shape_description: "9 characters, then the letter B, then 2 digits",
        shape: |b| {
            b.len() == 12
                && b[..9].bytes().all(|c| c.is_ascii_alphanumeric())
                && b.as_bytes()[9] == b'B'
                && all_digits(&b[10..])
        },
        checksum: Some(nl_checksum),
    },
    CountryRules {
        prefix: "PL",
        shape_description: "10 digits",
        shape: |b| b.len() == 10 && all_digits(b),
        checksum: Some(pl_checksum),
    },
    CountryRules {
        prefix: "PT",
        shape_description: "9 digits",
        shape: |b| b.len() == 9 && all_digits(b),
        checksum: Some(pt_checksum),
    },
    CountryRules {
        prefix: "RO",
        shape_description: "2 to 10 digits",
        shape: |b| (2..=10).contains(&b.len()) && all_digits(b),
        checksum: None,
    },
    CountryRules {
        prefix: "SE",
        shape_description: "12 digits",
        shape: |b| b.len() == 12 && all_digits(b),
        checksum: Some(se_checksum),
    },
    CountryRules {
        prefix: "SI",
        shape_description: "8 digits",
        shape: |b| b.len() == 8 && all_digits(b),
        checksum: Some(si_checksum),
    },
    CountryRules {
        prefix: "SK",
        shape_description: "10 digits",
        shape: |b| b.len() == 10 && all_digits(b),
        checksum: Some(sk_checksum),
    },
];

// ---- checksum algorithms ----------------------------------------------------
//
// Every one of these is pinned by a real, independently-known VAT id in the
// tests below, so a transcription slip fails the suite rather than a customer.

fn all_digits(s: &str) -> bool {
    !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit())
}

/// The decimal digits of `s`. Callers check the shape first, so the only
/// characters skipped are the fixed letters in a shape (`U`, `B`).
fn digits(s: &str) -> Vec<u32> {
    s.chars().filter_map(|c| c.to_digit(10)).collect()
}

/// Σ dᵢ × wᵢ over as many digits as there are weights.
fn weighted(d: &[u32], weights: &[u32]) -> u32 {
    d.iter().zip(weights).map(|(digit, w)| digit * w).sum()
}

/// The value of a digit string modulo `m`, without ever building an integer
/// that could overflow.
fn digits_mod(d: &[u32], m: u64) -> u64 {
    d.iter()
        .fold(0, |acc, digit| (acc * 10 + u64::from(*digit)) % m)
}

/// The Luhn (ISO/IEC 7812) check over a digit string that includes its own
/// check digit as the last element.
fn luhn(d: &[u32]) -> bool {
    let sum: u32 = d
        .iter()
        .rev()
        .enumerate()
        .map(|(i, digit)| {
            if i % 2 == 1 {
                let doubled = digit * 2;
                doubled / 10 + doubled % 10
            } else {
                *digit
            }
        })
        .sum();
    sum.is_multiple_of(10)
}

/// Austria — alternating 1/2 weighting with digit sums, check = (96 − Σ) mod 10.
fn at_checksum(body: &str) -> bool {
    let d = digits(body);
    // Seven digits at most nine each, so the sum can never reach 96 and the
    // subtraction below cannot go negative.
    let sum: u32 = d[..7]
        .iter()
        .enumerate()
        .map(|(i, digit)| {
            if i % 2 == 1 {
                let doubled = digit * 2;
                doubled / 10 + doubled % 10
            } else {
                *digit
            }
        })
        .sum();
    (96 - sum) % 10 == d[7]
}

/// Belgium — the last two digits complete the first eight to a multiple of 97.
fn be_checksum(body: &str) -> bool {
    let d = digits(body);
    (digits_mod(&d[..8], 97) + digits_mod(&d[8..], 97)).is_multiple_of(97)
}

/// Germany — ISO 7064 MOD 11,10, the USt-IdNr procedure.
fn de_checksum(body: &str) -> bool {
    let d = digits(body);
    let mut p = 10;
    for digit in &d[..8] {
        let m = (digit + p) % 10;
        let m = if m == 0 { 10 } else { m };
        p = (2 * m) % 11;
    }
    let check = if p == 1 { 0 } else { 11 - p };
    check == d[8]
}

/// Denmark — weighted sum divisible by 11 (the check digit carries its own
/// weight of 1, so there is nothing to compare against).
fn dk_checksum(body: &str) -> bool {
    weighted(&digits(body), &[2, 7, 6, 5, 4, 3, 2, 1]).is_multiple_of(11)
}

/// Finland — weighted mod 11; a remainder of 1 has no valid check digit.
fn fi_checksum(body: &str) -> bool {
    let d = digits(body);
    let rest = weighted(&d, &[7, 9, 10, 5, 8, 4, 2]) % 11;
    match rest {
        0 => d[7] == 0,
        1 => false,
        _ => 11 - rest == d[7],
    }
}

/// France — the two-digit key is `(12 + 3 × (SIREN mod 97)) mod 97`. The
/// alphanumeric keys issued since 2014 use an unpublished variant, so those
/// pass on shape alone rather than being wrongly refused.
fn fr_checksum(body: &str) -> bool {
    let (key, siren) = body.split_at(2);
    if !all_digits(key) {
        return true;
    }
    let key = digits_mod(&digits(key), 100);
    let siren = digits_mod(&digits(siren), 97);
    key == (12 + 3 * siren) % 97
}

/// Italy — Luhn over all eleven digits.
fn it_checksum(body: &str) -> bool {
    luhn(&digits(body))
}

/// Luxembourg — the first six digits mod 89 are the last two.
fn lu_checksum(body: &str) -> bool {
    let d = digits(body);
    digits_mod(&d[..6], 89) == digits_mod(&d[6..], 100)
}

/// The Netherlands — the classic "elfproef" over the nine-digit BSN/RSIN.
///
/// The identifiers issued to sole traders since 2020 are not derived from a
/// BSN and carry letters in that block; the algorithm behind them is not
/// published in a form we can pin to a known-good sample, so they pass on
/// shape alone. Refusing them would make real one-person companies
/// un-invoiceable, which is the worse failure.
fn nl_checksum(body: &str) -> bool {
    let block = &body[..9];
    if !all_digits(block) {
        return true;
    }
    let d = digits(block);
    let sum = i64::from(weighted(&d[..8], &[9, 8, 7, 6, 5, 4, 3, 2])) - i64::from(d[8]);
    sum > 0 && sum % 11 == 0
}

/// Poland — the NIP weighted mod 11; a remainder of 10 has no valid check.
fn pl_checksum(body: &str) -> bool {
    let d = digits(body);
    let rest = weighted(&d[..9], &[6, 5, 7, 2, 3, 4, 5, 6, 7]) % 11;
    rest != 10 && rest == d[9]
}

/// Portugal — weighted mod 11, remainders 0 and 1 meaning a check digit of 0.
fn pt_checksum(body: &str) -> bool {
    let d = digits(body);
    let rest = weighted(&d[..8], &[9, 8, 7, 6, 5, 4, 3, 2]) % 11;
    let check = if rest < 2 { 0 } else { 11 - rest };
    check == d[8]
}

/// Sweden — Luhn over the ten-digit organisation number; the trailing two
/// digits are the branch counter and carry no check.
fn se_checksum(body: &str) -> bool {
    luhn(&digits(body)[..10])
}

/// Slovenia — weighted mod 11; a remainder of 0 has no valid check digit.
fn si_checksum(body: &str) -> bool {
    let d = digits(body);
    let rest = weighted(&d[..7], &[8, 7, 6, 5, 4, 3, 2]) % 11;
    match rest {
        0 => false,
        1 => d[7] == 0,
        _ => 11 - rest == d[7],
    }
}

/// Slovakia — the whole ten-digit number is divisible by 11.
fn sk_checksum(body: &str) -> bool {
    digits_mod(&digits(body), 11) == 0
}

#[cfg(test)]
mod tests {
    use super::*;

    /// One member state: a real, independently-known VAT id that must pass,
    /// and ids that must not. Every entry with a checksum carries at least one
    /// rejection that is *only* a check-digit failure, so a transcription slip
    /// in the algorithm cannot pass unnoticed.
    struct Case {
        country: &'static str,
        valid: &'static str,
        rejected: &'static [&'static str],
    }

    const CASES: &[Case] = &[
        Case {
            country: "AT",
            valid: "ATU13585627",
            rejected: &["ATU1358562", "ATU13585628", "AT13585627"],
        },
        Case {
            country: "BE",
            valid: "BE0776091951",
            rejected: &["BE0776091952", "BE2776091951", "BE077609195"],
        },
        Case {
            country: "BG",
            valid: "BG123456789",
            rejected: &["BG12345678", "BG12345678901"],
        },
        Case {
            country: "CY",
            valid: "CY12345678L",
            rejected: &["CY123456789", "CY1234567L"],
        },
        Case {
            country: "CZ",
            valid: "CZ12345678",
            rejected: &["CZ1234567", "CZ12345678901"],
        },
        Case {
            country: "DE",
            valid: "DE811907980",
            rejected: &["DE811907981", "DE81190798", "DE8119079801"],
        },
        Case {
            country: "DK",
            valid: "DK13585628",
            rejected: &["DK13585629", "DK1358562"],
        },
        Case {
            country: "EE",
            valid: "EE100931558",
            rejected: &["EE10093155"],
        },
        // Greece invoices under the EL prefix while its country code is GR.
        Case {
            country: "GR",
            valid: "EL094259216",
            rejected: &["EL09425921"],
        },
        Case {
            country: "ES",
            valid: "ESA12345674",
            rejected: &["ES123456789", "ESA1234567"],
        },
        Case {
            country: "FI",
            valid: "FI20774740",
            rejected: &["FI20774741", "FI2077474"],
        },
        Case {
            country: "FR",
            valid: "FR40303265045",
            rejected: &["FR41303265045", "FR4030326504", "FRIO303265045"],
        },
        Case {
            country: "HR",
            valid: "HR12345678901",
            rejected: &["HR1234567890"],
        },
        Case {
            country: "HU",
            valid: "HU12345678",
            rejected: &["HU1234567"],
        },
        Case {
            country: "IE",
            valid: "IE1234567FA",
            rejected: &["IE123456", "IEA1234567"],
        },
        Case {
            country: "IT",
            valid: "IT00743110157",
            rejected: &["IT00743110158", "IT0074311015"],
        },
        Case {
            country: "LT",
            valid: "LT123456789",
            rejected: &["LT1234567890"],
        },
        Case {
            country: "LU",
            valid: "LU15027442",
            rejected: &["LU15027443", "LU1502744"],
        },
        Case {
            country: "LV",
            valid: "LV12345678901",
            rejected: &["LV1234567890"],
        },
        Case {
            country: "MT",
            valid: "MT12345678",
            rejected: &["MT1234567"],
        },
        Case {
            country: "NL",
            valid: "NL004495445B01",
            rejected: &["NL004495446B01", "NL004495445X01", "NL004495445B0"],
        },
        Case {
            country: "PL",
            valid: "PL5260250995",
            rejected: &["PL5260250996", "PL526025099"],
        },
        Case {
            country: "PT",
            valid: "PT502011378",
            rejected: &["PT502011379", "PT50201137"],
        },
        Case {
            country: "RO",
            valid: "RO1234567890",
            rejected: &["RO12345678901", "RO1"],
        },
        Case {
            country: "SE",
            valid: "SE123456789701",
            rejected: &["SE123456789801", "SE12345678970"],
        },
        Case {
            country: "SI",
            valid: "SI50223054",
            rejected: &["SI50223055", "SI5022305"],
        },
        Case {
            country: "SK",
            valid: "SK2020270780",
            rejected: &["SK2020270781", "SK202027078"],
        },
    ];

    #[test]
    fn every_member_state_accepts_its_own_real_id() {
        for case in CASES {
            assert_eq!(
                canonicalize(case.valid, case.country),
                Ok(Some(case.valid.to_owned())),
                "{} rejected its own valid id {}",
                case.country,
                case.valid
            );
        }
    }

    #[test]
    fn malformed_and_mistyped_ids_are_refused() {
        for case in CASES {
            for bad in case.rejected {
                assert!(
                    canonicalize(bad, case.country).is_err(),
                    "{} accepted {bad}, which is not a valid id",
                    case.country
                );
            }
        }
    }

    #[test]
    fn the_prefix_is_optional_on_input_and_always_present_on_output() {
        for case in CASES {
            let bare = &case.valid[2..];
            assert_eq!(
                canonicalize(bare, case.country),
                Ok(Some(case.valid.to_owned())),
                "{} did not accept its id without the prefix",
                case.country
            );
        }
    }

    #[test]
    fn separators_and_case_are_presentation_only() {
        for written in [
            "DE 811.907-980",
            "de811907980",
            " DE811907980 ",
            "811 907 980",
            "de 811.907.980",
        ] {
            assert_eq!(
                canonicalize(written, "DE"),
                Ok(Some("DE811907980".to_owned())),
                "{written:?} did not canonicalise"
            );
        }
    }

    #[test]
    fn an_empty_id_is_a_b2c_customer_not_an_error() {
        for blank in ["", "   ", "\t", "--", ". .", "\u{a0}"] {
            assert_eq!(canonicalize(blank, "DE"), Ok(None), "{blank:?}");
        }
    }

    #[test]
    fn a_foreign_registration_is_kept_as_written() {
        // A German customer invoicing under a French registration, and the
        // other way round: both are real and both must survive.
        assert_eq!(
            canonicalize("FR40303265045", "DE"),
            Ok(Some("FR40303265045".to_owned()))
        );
        assert_eq!(
            canonicalize("DE811907980", "FR"),
            Ok(Some("DE811907980".to_owned()))
        );
    }

    #[test]
    fn a_broken_foreign_id_reports_the_country_it_names() {
        // Typed for a German customer, but the id names France: the error
        // must be about France, not about German 9-digit ids.
        assert_eq!(
            canonicalize("FR41303265045", "DE"),
            Err(VatIdError::Checksum("FR"))
        );
        assert_eq!(
            canonicalize("FR4030326504", "DE"),
            Err(VatIdError::Shape(
                "FR",
                "a 2-character key (no I or O) followed by 9 digits"
            ))
        );
    }

    #[test]
    fn a_french_key_that_looks_like_a_country_code_is_still_french() {
        // "BE" is a legal French alphanumeric key; it must not be read as a
        // Belgian id and refused for having 9 digits instead of 10.
        assert_eq!(
            canonicalize("BE303265045", "FR"),
            Ok(Some("FRBE303265045".to_owned()))
        );
    }

    #[test]
    fn countries_without_published_rules_pass_on_charset_and_length() {
        assert_eq!(
            canonicalize("CHE-116.281.710", "CH"),
            Ok(Some("CHE116281710".to_owned()))
        );
        assert_eq!(
            canonicalize("123456789", "US"),
            Ok(Some("123456789".to_owned()))
        );
        // …but an id that names an EU country must still be valid for it.
        assert_eq!(
            canonicalize("DE811907981", "CH"),
            Err(VatIdError::Checksum("DE"))
        );
    }

    #[test]
    fn charset_and_length_are_enforced_before_anything_else() {
        assert_eq!(canonicalize("DE!!907980", "DE"), Err(VatIdError::Charset));
        assert_eq!(
            canonicalize("DE811907980/X", "DE"),
            Err(VatIdError::Charset)
        );
        assert_eq!(
            canonicalize(&"9".repeat(VAT_ID_MAX_CHARS + 1), "DE"),
            Err(VatIdError::TooLong)
        );
        assert!(canonicalize(&"9".repeat(VAT_ID_MAX_CHARS), "US").is_ok());
    }

    #[test]
    fn errors_never_contain_the_id() {
        let secret = "DE811907981";
        let Err(error) = canonicalize(secret, "DE") else {
            panic!("expected the mistyped id to be refused");
        };
        assert!(!error.to_string().contains(secret));
        assert!(!error.to_string().contains("811"));
    }

    #[test]
    fn dutch_sole_trader_ids_are_accepted_on_shape() {
        // Post-2020 Dutch ids carry letters in the first block and are not
        // BSN-derived; the elfproef does not apply and must not be run.
        assert_eq!(
            canonicalize("NL123456789B01", "NL"),
            Err(VatIdError::Checksum("NL")),
            "an all-digit block is still checked"
        );
        assert_eq!(
            canonicalize("NL12A456789B01", "NL"),
            Ok(Some("NL12A456789B01".to_owned()))
        );
    }
}
