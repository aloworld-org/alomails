//! Field rules shared by every alo Billing record (ADR 0035, wave B1).
//!
//! Customers, products, invoice lines and quote lines all bound the same
//! handful of primitive shapes — a trimmed string, a required name, an amount
//! in integer cents, a VAT rate in basis points. They live here so a rule is
//! stated once and every billing module answers a caller identically; a
//! module keeps only the rules that are genuinely its own (a customer's VAT
//! id, a product's unit).
//!
//! Every function is pure and returns [`StoreError::Validation`] naming the
//! violated rule — never echoing the value, which may be customer data
//! (law 1).

use crate::error::{Result, StoreError};

/// The highest VAT rate we accept, in basis points: 10 000 bp = 100 %.
///
/// No member state levies anywhere near this, but the ceiling is the one that
/// is *definitionally* true rather than a guess at fiscal policy, so a rate
/// change in any member state can never make us reject a real invoice.
pub const VAT_RATE_MAX_BP: i32 = 10_000;

/// The highest unit price we accept, in cents: €10 000 000.00 per unit.
///
/// This is a typo guard with an arithmetic job. Line net is
/// `qty_milli × unit_price_cents / 1000` (B1.06); capping the price at 10^9
/// cents keeps that product inside `i64` for any quantity the line model can
/// hold, so no document total can ever overflow into a wrong number.
pub const UNIT_PRICE_MAX_CENTS: i64 = 1_000_000_000;

/// Longest payment terms we accept, in days (a year of credit is already far
/// beyond any real B2B term; anything longer is a typo).
pub const PAYMENT_TERMS_MAX_DAYS: i32 = 365;
/// Payment terms applied when the caller states none — the EU B2B default.
pub const DEFAULT_PAYMENT_TERMS_DAYS: i32 = 30;
/// Currency applied when the caller states none.
pub const DEFAULT_CURRENCY: &str = "EUR";

/// Trims `value` and rejects it if it exceeds `max` characters.
///
/// Counts characters, not bytes: a 200-character limit means 200 characters
/// of any script, so a name in Greek is not half the length of one in ASCII.
pub(crate) fn bounded(field: &str, value: &str, max: usize) -> Result<String> {
    let trimmed = value.trim();
    if trimmed.chars().count() > max {
        return Err(StoreError::Validation(format!(
            "{field} must be at most {max} characters"
        )));
    }
    Ok(trimmed.to_owned())
}

/// [`bounded`], and additionally non-blank — for a field the record cannot
/// meaningfully exist without (a customer's name, a product's name).
pub(crate) fn required(field: &str, value: &str, max: usize) -> Result<String> {
    let value = bounded(field, value, max)?;
    if value.is_empty() {
        return Err(StoreError::Validation(format!("{field} must not be empty")));
    }
    Ok(value)
}

/// Validates a VAT rate in basis points (2100 = 21 %). Zero is valid and
/// common: exempt, reverse-charge and intra-Community supplies are all 0 %.
pub(crate) fn vat_rate_bp(value: i32) -> Result<i32> {
    if !(0..=VAT_RATE_MAX_BP).contains(&value) {
        return Err(StoreError::Validation(format!(
            "VAT rate must be between 0 and {VAT_RATE_MAX_BP} basis points"
        )));
    }
    Ok(value)
}

/// Validates a non-negative amount in integer cents against
/// [`UNIT_PRICE_MAX_CENTS`].
///
/// Negative prices are refused here on purpose: a discount is a negative
/// *quantity* or a credit note (B1.09), both of which stay auditable, whereas
/// a negative price hides a refund inside an ordinary invoice line.
pub(crate) fn unit_price_cents(field: &str, value: i64) -> Result<i64> {
    if !(0..=UNIT_PRICE_MAX_CENTS).contains(&value) {
        return Err(StoreError::Validation(format!(
            "{field} must be between 0 and {UNIT_PRICE_MAX_CENTS} cents"
        )));
    }
    Ok(value)
}

/// Validates an ISO 4217 currency code, returning it uppercased.
///
/// Shape only — three ASCII letters. The store deliberately does not carry a
/// list of assigned codes: that list changes under us and a rejected valid
/// code blocks a real invoice, while the FX table (B1.21) is what decides
/// which codes a tenant can actually invoice in.
pub(crate) fn currency(value: &str) -> Result<String> {
    let value = value.trim();
    if value.len() != 3 || !value.bytes().all(|b| b.is_ascii_alphabetic()) {
        return Err(StoreError::Validation(
            "currency must be a three-letter ISO 4217 code".to_owned(),
        ));
    }
    Ok(value.to_ascii_uppercase())
}

/// Validates an ISO 3166-1 alpha-2 country code, returning it uppercased.
///
/// Shape only: two ASCII letters. The store deliberately does not carry a
/// list of assigned codes — that list changes under us and a rejected valid
/// code blocks a real customer, while a two-letter typo is caught by the VAT
/// rules ([`crate::vat_id`]) the moment it matters.
///
/// Blank is refused here. A field where "unstated" is legitimate (the
/// issuer's own country, [`crate::billing_settings`]) checks for blank first;
/// on a customer, the country decides VAT treatment and is required.
pub(crate) fn country(value: &str) -> Result<String> {
    let value = value.trim();
    if value.len() != 2 || !value.bytes().all(|b| b.is_ascii_alphabetic()) {
        return Err(StoreError::Validation(
            "country must be a two-letter ISO 3166-1 code".to_owned(),
        ));
    }
    Ok(value.to_ascii_uppercase())
}

/// RFC 5321 caps a path at 256 octets; 320 is the everyday `local@domain`
/// ceiling and is what every validator in the wild uses.
pub const EMAIL_MAX_CHARS: usize = 320;

/// Validates an optional address a document is sent to: one `@`, non-empty
/// local and domain parts, a dot in the domain, no whitespace, bounded.
///
/// Blank is `None` rather than an error — a customer whose invoice is printed
/// and a supplier who takes orders by phone are both ordinary. Deliberately
/// shape-only: the authority on whether an address exists is the SMTP
/// conversation, and a stricter grammar here would reject real addresses.
pub(crate) fn email(value: Option<&str>) -> Result<Option<String>> {
    let Some(raw) = value else { return Ok(None) };
    let raw = raw.trim();
    if raw.is_empty() {
        return Ok(None);
    }
    if raw.chars().count() > EMAIL_MAX_CHARS {
        return Err(StoreError::Validation(format!(
            "email must be at most {EMAIL_MAX_CHARS} characters"
        )));
    }
    let mut parts = raw.split('@');
    let ok = match (parts.next(), parts.next(), parts.next()) {
        (Some(local), Some(domain), None) => {
            !local.is_empty()
                && !domain.is_empty()
                && domain.contains('.')
                && !domain.starts_with('.')
                && !domain.ends_with('.')
                && !raw.chars().any(char::is_whitespace)
        }
        _ => false,
    };
    if !ok {
        return Err(StoreError::Validation(
            "email must be a single address of the form local@domain".to_owned(),
        ));
    }
    Ok(Some(raw.to_owned()))
}

/// Validates payment terms in days: how long after issue an invoice is due.
/// Zero is valid — "due on receipt".
pub(crate) fn payment_terms_days(value: i32) -> Result<i32> {
    if !(0..=PAYMENT_TERMS_MAX_DAYS).contains(&value) {
        return Err(StoreError::Validation(format!(
            "payment terms must be between 0 and {PAYMENT_TERMS_MAX_DAYS} days"
        )));
    }
    Ok(value)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn message<T: std::fmt::Debug>(result: Result<T>) -> String {
        match result {
            Err(StoreError::Validation(msg)) => msg,
            other => panic!("expected Validation, got {other:?}"),
        }
    }

    #[test]
    fn bounded_trims_and_counts_characters_not_bytes() {
        assert_eq!(bounded("name", "  Acme  ", 10).unwrap_or_default(), "Acme");
        // Five two-byte characters fit a five-character bound.
        assert_eq!(bounded("name", "Ωμέγα", 5).unwrap_or_default(), "Ωμέγα");
        assert!(message(bounded("name", "Ωμέγα", 4)).contains("at most 4"));
        // Trimming happens before measuring: padding never costs a caller.
        assert!(bounded("name", "    abc    ", 3).is_ok());
    }

    #[test]
    fn required_rejects_blank_but_keeps_the_bound() {
        for blank in ["", "   ", "\t\n"] {
            assert!(message(required("name", blank, 10)).contains("must not be empty"));
        }
        assert!(message(required("name", "abcdef", 3)).contains("at most"));
        assert_eq!(required("name", " ok ", 10).unwrap_or_default(), "ok");
    }

    #[test]
    fn vat_rate_spans_zero_to_one_hundred_percent() {
        for ok in [0, 600, 2100, VAT_RATE_MAX_BP] {
            assert_eq!(vat_rate_bp(ok).unwrap_or_default(), ok);
        }
        for bad in [-1, VAT_RATE_MAX_BP + 1, i32::MIN, i32::MAX] {
            assert!(
                matches!(vat_rate_bp(bad), Err(StoreError::Validation(_))),
                "expected rejection: {bad}"
            );
        }
    }

    #[test]
    fn unit_price_is_non_negative_and_capped() {
        for ok in [0, 1, 1_250, UNIT_PRICE_MAX_CENTS] {
            assert_eq!(unit_price_cents("unit price", ok).unwrap_or_default(), ok);
        }
        for bad in [-1, UNIT_PRICE_MAX_CENTS + 1, i64::MIN, i64::MAX] {
            assert!(
                matches!(
                    unit_price_cents("unit price", bad),
                    Err(StoreError::Validation(_))
                ),
                "expected rejection: {bad}"
            );
        }
    }

    #[test]
    fn currency_must_be_three_letters() {
        for ok in ["eur", "USD", " chf "] {
            assert!(currency(ok).is_ok(), "expected valid: {ok:?}");
        }
        assert_eq!(currency("eur").unwrap_or_default(), "EUR");
        for bad in ["", "EU", "EURO", "EU1", "€"] {
            assert!(
                matches!(currency(bad), Err(StoreError::Validation(_))),
                "expected rejection: {bad:?}"
            );
        }
    }

    #[test]
    fn country_must_be_two_letters() {
        for ok in ["de", "DE", "Fr", " nl "] {
            assert!(country(ok).is_ok(), "expected valid: {ok:?}");
        }
        assert_eq!(country("be").unwrap_or_default(), "BE");
        // Blank is refused here: a field where "unstated" is legitimate
        // checks for it before calling.
        for bad in ["", "   ", "D", "DEU", "D1", "12", "d€"] {
            assert!(
                matches!(country(bad), Err(StoreError::Validation(_))),
                "expected rejection: {bad:?}"
            );
        }
    }

    #[test]
    fn payment_terms_are_ranged() {
        // Zero is "due on receipt", not a missing value.
        for ok in [0, 14, 30, PAYMENT_TERMS_MAX_DAYS] {
            assert_eq!(payment_terms_days(ok).unwrap_or(-1), ok);
        }
        for bad in [-1, PAYMENT_TERMS_MAX_DAYS + 1, i32::MIN, i32::MAX] {
            assert!(
                matches!(payment_terms_days(bad), Err(StoreError::Validation(_))),
                "expected rejection: {bad}"
            );
        }
    }

    #[test]
    fn an_email_is_optional_but_well_formed_when_present() {
        // Absent and blank are the same fact — no address was given, which is
        // ordinary for a printed invoice and for a supplier taking orders by
        // phone.
        assert_eq!(email(None).unwrap_or_default(), None);
        assert_eq!(email(Some("   ")).unwrap_or_default(), None);
        assert_eq!(
            email(Some("  a@b.test ")).unwrap_or_default(),
            Some("a@b.test".to_owned()),
            "trimmed, so the same address never stores two ways"
        );
        for bad in [
            "no-at-sign",
            "@domain.test",
            "local@",
            "two@at@signs.test",
            "local@nodot",
            "local@.leading.test",
            "local@trailing.",
            "spa ce@domain.test",
        ] {
            assert!(
                matches!(email(Some(bad)), Err(StoreError::Validation(_))),
                "expected rejection: {bad:?}"
            );
        }
        let too_long = format!("{}@domain.test", "x".repeat(EMAIL_MAX_CHARS));
        assert!(message(email(Some(&too_long))).contains("at most"));
        // The message names the rule and never the address itself (law 1).
        let refused = message(email(Some("two@at@signs.test")));
        assert!(refused.contains("email"), "{refused}");
        assert!(!refused.contains("signs.test"), "{refused}");
    }

    #[test]
    fn the_price_cap_keeps_line_arithmetic_inside_i64() {
        // B1.06 computes `qty_milli × unit_price_cents`. At the ceiling of
        // both, that product must still be an i64 — otherwise a total could
        // silently wrap. A million units of the dearest possible item:
        let qty_milli: i64 = 1_000_000_000;
        assert!(qty_milli.checked_mul(UNIT_PRICE_MAX_CENTS).is_some());
    }
}
