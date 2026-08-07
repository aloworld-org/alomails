//! The issuer side of every billing document (alo Billing, ADR 0035, wave
//! B1) — who is invoicing, under which numbers, and where the money goes.
//!
//! A customer record ([`crate::billing_customers`]) says who a document is
//! *to*; this says who it is *from*, and it is the same for every document a
//! tenant raises. So there is **one row per tenant**, keyed by the tenant
//! itself, reached through the account door like every other billing record.
//!
//! Two rules are specific to this record:
//!
//! - **A tenant that has never saved reads the blanks, not a `NotFound`.**
//!   The issuer identity conceptually always exists — it is simply unstated —
//!   and a print view that had to ask "has billing been set up?" would be a
//!   second source of truth about a record with exactly one row. [`Default`]
//!   is that unstated state, and [`BillingSettings::is_stated`] is how a
//!   caller tells it apart from a saved one.
//! - **The bank details are held to their own standard.** The IBAN goes
//!   through [`crate::iban`] — country length *and* ISO 7064 mod-97 check —
//!   because a typo'd account number is a payment that never arrives and is
//!   caught at the point of entry or not at all.
//!
//! Nothing here is snapshotted onto a document when it is issued. Reprinting
//! last year's invoice therefore shows the *current* address and bank, which
//! is what moving office or changing bank is supposed to do; the facts that
//! must never drift (the number, the dates, the lines, the money) live on the
//! document itself.

use time::OffsetDateTime;

use crate::account::AccountStore;
use crate::billing_field::{bounded, country as country_code, required};
use crate::error::{Result, StoreError};
use crate::{iban, vat_id};

/// The issuer's legal name is a company name — generous but bounded.
pub const LEGAL_NAME_MAX_CHARS: usize = 200;

const ADDRESS_LINE_MAX_CHARS: usize = 200;
const POSTAL_CODE_MAX_CHARS: usize = 20;
const CITY_MAX_CHARS: usize = 120;
const REGISTRATION_MAX_CHARS: usize = 60;
const EMAIL_MAX_CHARS: usize = 320;
const PHONE_MAX_CHARS: usize = 40;
const WEBSITE_MAX_CHARS: usize = 200;
const BANK_NAME_MAX_CHARS: usize = 120;
/// The footer prints under the totals of every document: a couple of
/// sentences of terms, not an appendix.
pub const FOOTER_NOTE_MAX_CHARS: usize = 500;

/// The columns every read selects, in `SettingsRow` order.
const SETTINGS_COLS: &str = "legal_name, address_line1, address_line2, postal_code, city, \
     country, vat_id, registration_no, email, phone, website, iban, bic, bank_name, \
     account_holder, footer_note, updated_by, updated_at";

/// The writable shape of the issuer identity. A save is a **full replace** —
/// the route layer merges a partial `PATCH` onto the stored record first, the
/// same convention as customers and products.
///
/// [`Default`] is the unstated identity: every field blank. It is deliberately
/// *not* savable as such — [`AccountStore::save_billing_settings`] requires a
/// legal name, because a document that does not name its issuer is not an
/// invoice.
#[derive(Debug, Clone, Default)]
pub struct NewBillingSettings {
    /// The legal name the tenant invoices under. Required, non-blank.
    pub legal_name: String,
    /// Street address, first line.
    pub address_line1: String,
    /// Street address, second line.
    pub address_line2: String,
    /// Postal/ZIP code.
    pub postal_code: String,
    /// City / town.
    pub city: String,
    /// ISO 3166-1 alpha-2 country code, or blank while unstated.
    pub country: String,
    /// VAT identification number, or `None` for a tenant not registered.
    pub vat_id: Option<String>,
    /// Company-register number as printed (KVK, SIREN, HRB, …).
    pub registration_no: String,
    /// Billing contact address printed on the document.
    pub email: String,
    /// Billing contact telephone.
    pub phone: String,
    /// Website, as printed.
    pub website: String,
    /// Where the money goes, or `None` while unstated.
    pub iban: Option<String>,
    /// The IBAN's BIC/SWIFT code, or `None`.
    pub bic: Option<String>,
    /// The bank's name, when it should be printed.
    pub bank_name: String,
    /// The account holder, when it is not simply the legal name.
    pub account_holder: String,
    /// A line under the totals: retention of title, late-payment terms.
    pub footer_note: String,
}

/// The stored issuer identity. Every field is the canonical form: country and
/// VAT id uppercase and prefixed, IBAN and BIC compacted and uppercase.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct BillingSettings {
    /// The legal name the tenant invoices under; blank only when unstated.
    pub legal_name: String,
    /// Street address, first line.
    pub address_line1: String,
    /// Street address, second line.
    pub address_line2: String,
    /// Postal/ZIP code.
    pub postal_code: String,
    /// City / town.
    pub city: String,
    /// ISO 3166-1 alpha-2 country code, uppercase; blank while unstated.
    pub country: String,
    /// VAT identification number in canonical prefixed form.
    pub vat_id: Option<String>,
    /// Company-register number as printed.
    pub registration_no: String,
    /// Billing contact address.
    pub email: String,
    /// Billing contact telephone.
    pub phone: String,
    /// Website, as printed.
    pub website: String,
    /// IBAN, compacted and uppercase.
    pub iban: Option<String>,
    /// BIC/SWIFT, compacted and uppercase.
    pub bic: Option<String>,
    /// The bank's name.
    pub bank_name: String,
    /// The account holder, when it is not the legal name.
    pub account_holder: String,
    /// The line printed under the totals of every document.
    pub footer_note: String,
    /// The user who last saved, or `None` while the identity is unstated.
    pub updated_by: Option<String>,
    /// When it was last saved, or `None` while unstated.
    pub updated_at: Option<OffsetDateTime>,
}

impl BillingSettings {
    /// Whether the tenant has ever saved its issuer identity.
    ///
    /// The blanks are a real, readable state, so this is how a caller that
    /// cares — a print view deciding whether to prompt, a future e-invoice
    /// writer that cannot proceed without a VAT id — tells "not yet stated"
    /// from "stated, and empty in this field".
    pub fn is_stated(&self) -> bool {
        self.updated_at.is_some()
    }

    /// The name the bank account is held in: the stated holder, or the legal
    /// name, which is what it is for all but a trading-name account.
    pub fn effective_account_holder(&self) -> &str {
        if self.account_holder.is_empty() {
            &self.legal_name
        } else {
            &self.account_holder
        }
    }
}

/// A validated, normalised identity ready to be bound into the upsert.
#[derive(Debug)]
struct Normalized {
    legal_name: String,
    address_line1: String,
    address_line2: String,
    postal_code: String,
    city: String,
    country: String,
    vat_id: Option<String>,
    registration_no: String,
    email: String,
    phone: String,
    website: String,
    iban: Option<String>,
    bic: Option<String>,
    bank_name: String,
    account_holder: String,
    footer_note: String,
}

/// Validates the issuer's own country, where blank is legitimate.
///
/// Unlike a customer's country — which decides their VAT treatment and is
/// required — a tenant may reasonably save a name and an address before
/// stating anything fiscal. Blank stays blank; anything else has to be a
/// two-letter code.
fn optional_country(value: &str) -> Result<String> {
    let value = value.trim();
    if value.is_empty() {
        return Ok(String::new());
    }
    country_code(value)
}

/// Validates and canonicalises the issuer's own VAT id.
///
/// Held to the same published shape and check digit as a customer's, against
/// the **issuer's** country — with one difference: a VAT id cannot be judged
/// without one, so stating a VAT id while leaving the country blank is the
/// error, rather than a number nothing checked.
fn normalize_vat_id(vat_id: Option<&str>, country: &str) -> Result<Option<String>> {
    let stated = vat_id.map(str::trim).filter(|v| !v.is_empty());
    let Some(raw) = stated else { return Ok(None) };
    if country.is_empty() {
        return Err(StoreError::Validation(
            "state the country before the VAT id: a VAT id is checked against its member state"
                .to_owned(),
        ));
    }
    vat_id::canonicalize(raw, country).map_err(|error| StoreError::Validation(error.to_string()))
}

/// Validates an optional billing contact address — the same rule a customer's
/// email is held to, stated positively: one `@`, a dotted domain, no spaces.
fn validate_email(value: &str) -> Result<String> {
    let raw = value.trim();
    if raw.is_empty() {
        return Ok(String::new());
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
    Ok(raw.to_owned())
}

/// Validates and normalises the whole identity. Pure — no database, so the
/// rules are unit-tested directly.
///
/// The country is resolved first, because it is what the VAT id is judged
/// against; the bank details are last, because they are the only fields whose
/// refusal a user reads as "check what you typed against your bank statement"
/// rather than "fill this in".
fn normalize(input: &NewBillingSettings) -> Result<Normalized> {
    let country = optional_country(&input.country)?;
    Ok(Normalized {
        legal_name: required("legal name", &input.legal_name, LEGAL_NAME_MAX_CHARS)?,
        address_line1: bounded(
            "address line 1",
            &input.address_line1,
            ADDRESS_LINE_MAX_CHARS,
        )?,
        address_line2: bounded(
            "address line 2",
            &input.address_line2,
            ADDRESS_LINE_MAX_CHARS,
        )?,
        postal_code: bounded("postal code", &input.postal_code, POSTAL_CODE_MAX_CHARS)?,
        city: bounded("city", &input.city, CITY_MAX_CHARS)?,
        vat_id: normalize_vat_id(input.vat_id.as_deref(), &country)?,
        country,
        registration_no: bounded(
            "registration number",
            &input.registration_no,
            REGISTRATION_MAX_CHARS,
        )?,
        email: validate_email(&input.email)?,
        phone: bounded("phone", &input.phone, PHONE_MAX_CHARS)?,
        website: bounded("website", &input.website, WEBSITE_MAX_CHARS)?,
        iban: iban::canonicalize(input.iban.as_deref().unwrap_or_default())
            .map_err(|error| StoreError::Validation(error.to_string()))?,
        bic: iban::canonicalize_bic(input.bic.as_deref().unwrap_or_default())
            .map_err(|error| StoreError::Validation(error.to_string()))?,
        bank_name: bounded("bank name", &input.bank_name, BANK_NAME_MAX_CHARS)?,
        account_holder: bounded(
            "account holder",
            &input.account_holder,
            LEGAL_NAME_MAX_CHARS,
        )?,
        footer_note: bounded("footer note", &input.footer_note, FOOTER_NOTE_MAX_CHARS)?,
    })
}

// ---- row types --------------------------------------------------------------

/// The row shape every read decodes, in [`SETTINGS_COLS`] order.
#[derive(sqlx::FromRow)]
struct SettingsRow {
    legal_name: String,
    address_line1: String,
    address_line2: String,
    postal_code: String,
    city: String,
    country: String,
    vat_id: Option<String>,
    registration_no: String,
    email: String,
    phone: String,
    website: String,
    iban: Option<String>,
    bic: Option<String>,
    bank_name: String,
    account_holder: String,
    footer_note: String,
    updated_by: String,
    updated_at: OffsetDateTime,
}

impl SettingsRow {
    /// A stored row is by definition a *stated* identity, so `updated_by` and
    /// `updated_at` become `Some` here — the only place the two states of
    /// [`BillingSettings`] are told apart.
    fn into_settings(self) -> BillingSettings {
        BillingSettings {
            legal_name: self.legal_name,
            address_line1: self.address_line1,
            address_line2: self.address_line2,
            postal_code: self.postal_code,
            city: self.city,
            country: self.country,
            vat_id: self.vat_id,
            registration_no: self.registration_no,
            email: self.email,
            phone: self.phone,
            website: self.website,
            iban: self.iban,
            bic: self.bic,
            bank_name: self.bank_name,
            account_holder: self.account_holder,
            footer_note: self.footer_note,
            updated_by: Some(self.updated_by),
            updated_at: Some(self.updated_at),
        }
    }
}

impl AccountStore {
    /// The tenant's issuer identity, or the **blanks** when it has never been
    /// saved ([`BillingSettings::is_stated`] tells the two apart).
    ///
    /// # Errors
    /// [`StoreError::Db`] on failure. Never `NotFound`: the record always
    /// conceptually exists for a tenant that exists.
    pub async fn billing_settings(&self) -> Result<BillingSettings> {
        let row: Option<SettingsRow> = sqlx::query_as(&format!(
            "SELECT {SETTINGS_COLS} FROM billing_settings WHERE tenant_id = $1"
        ))
        .bind(self.tenant.as_str())
        .fetch_optional(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        Ok(row.map_or_else(BillingSettings::default, SettingsRow::into_settings))
    }

    /// Saves the tenant's issuer identity, creating the row on first save and
    /// replacing every field afterwards. Answers the stored record.
    ///
    /// # Errors
    /// [`StoreError::Validation`] on any field the caller can fix — a blank
    /// legal name, a bad country, a VAT id that fails its member state's
    /// check digit, an IBAN that fails its length or mod-97 check, a
    /// misshapen BIC; [`StoreError::Db`] on failure.
    pub async fn save_billing_settings(
        &self,
        input: &NewBillingSettings,
    ) -> Result<BillingSettings> {
        let s = normalize(input)?;
        let row: SettingsRow = sqlx::query_as(&format!(
            "INSERT INTO billing_settings (tenant_id, legal_name, address_line1, address_line2, \
                 postal_code, city, country, vat_id, registration_no, email, phone, website, \
                 iban, bic, bank_name, account_holder, footer_note, updated_by) \
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, \
                 $17, $18) \
             ON CONFLICT (tenant_id) DO UPDATE SET \
                 legal_name = EXCLUDED.legal_name, \
                 address_line1 = EXCLUDED.address_line1, \
                 address_line2 = EXCLUDED.address_line2, \
                 postal_code = EXCLUDED.postal_code, \
                 city = EXCLUDED.city, \
                 country = EXCLUDED.country, \
                 vat_id = EXCLUDED.vat_id, \
                 registration_no = EXCLUDED.registration_no, \
                 email = EXCLUDED.email, \
                 phone = EXCLUDED.phone, \
                 website = EXCLUDED.website, \
                 iban = EXCLUDED.iban, \
                 bic = EXCLUDED.bic, \
                 bank_name = EXCLUDED.bank_name, \
                 account_holder = EXCLUDED.account_holder, \
                 footer_note = EXCLUDED.footer_note, \
                 updated_by = EXCLUDED.updated_by, \
                 updated_at = now() \
             RETURNING {SETTINGS_COLS}"
        ))
        .bind(self.tenant.as_str())
        .bind(&s.legal_name)
        .bind(&s.address_line1)
        .bind(&s.address_line2)
        .bind(&s.postal_code)
        .bind(&s.city)
        .bind(&s.country)
        .bind(&s.vat_id)
        .bind(&s.registration_no)
        .bind(&s.email)
        .bind(&s.phone)
        .bind(&s.website)
        .bind(&s.iban)
        .bind(&s.bic)
        .bind(&s.bank_name)
        .bind(&s.account_holder)
        .bind(&s.footer_note)
        .bind(self.user.as_str())
        .fetch_one(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        Ok(row.into_settings())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn invalid<T: std::fmt::Debug>(result: Result<T>) -> String {
        match result {
            Err(StoreError::Validation(msg)) => msg,
            other => panic!("expected Validation, got {other:?}"),
        }
    }

    fn valid() -> NewBillingSettings {
        NewBillingSettings {
            legal_name: "Alo Werkplaats B.V.".to_owned(),
            country: "NL".to_owned(),
            ..Default::default()
        }
    }

    #[test]
    fn normalize_trims_and_canonicalises_every_form() {
        let input = NewBillingSettings {
            legal_name: "  Alo Werkplaats B.V. ".to_owned(),
            country: " nl ".to_owned(),
            city: " Amsterdam ".to_owned(),
            vat_id: Some(" nl 8123.45.678.B01 ".to_owned()),
            iban: Some("nl91 abna 0417 1643 00".to_owned()),
            bic: Some(" abnanl2a ".to_owned()),
            email: "  billing@alo.test ".to_owned(),
            ..Default::default()
        };
        let s = normalize(&input).unwrap_or_else(|e| panic!("rejected: {e}"));
        assert_eq!(s.legal_name, "Alo Werkplaats B.V.");
        assert_eq!(s.country, "NL");
        assert_eq!(s.city, "Amsterdam");
        assert_eq!(s.vat_id.as_deref(), Some("NL812345678B01"));
        assert_eq!(s.iban.as_deref(), Some("NL91ABNA0417164300"));
        assert_eq!(s.bic.as_deref(), Some("ABNANL2A"));
        assert_eq!(s.email, "billing@alo.test");
    }

    #[test]
    fn a_legal_name_is_the_one_required_field() {
        // Everything else may be unstated: a tenant can put its name on a
        // document before it has a VAT number or a bank account.
        let s = normalize(&NewBillingSettings {
            legal_name: "Sole Trader".to_owned(),
            ..Default::default()
        })
        .unwrap_or_else(|e| panic!("rejected: {e}"));
        assert_eq!(s.country, "");
        assert_eq!(s.vat_id, None);
        assert_eq!(s.iban, None);
        assert_eq!(s.bic, None);

        for blank in ["", "   ", "\t\n"] {
            let input = NewBillingSettings {
                legal_name: blank.to_owned(),
                ..valid()
            };
            assert!(invalid(normalize(&input)).contains("legal name"));
        }
        let input = NewBillingSettings {
            legal_name: "x".repeat(LEGAL_NAME_MAX_CHARS + 1),
            ..valid()
        };
        assert!(invalid(normalize(&input)).contains("at most"));
    }

    #[test]
    fn the_country_may_be_unstated_but_never_wrong() {
        for blank in ["", "  "] {
            let input = NewBillingSettings {
                country: blank.to_owned(),
                ..valid()
            };
            assert_eq!(normalize(&input).map(|s| s.country).unwrap_or_default(), "");
        }
        for bad in ["D", "DEU", "D1"] {
            let input = NewBillingSettings {
                country: bad.to_owned(),
                ..valid()
            };
            assert!(invalid(normalize(&input)).contains("country"));
        }
    }

    #[test]
    fn a_vat_id_without_a_country_cannot_be_checked_and_is_refused() {
        let input = NewBillingSettings {
            country: String::new(),
            vat_id: Some("NL812345678B01".to_owned()),
            ..valid()
        };
        assert!(invalid(normalize(&input)).contains("country before the VAT id"));

        // Blank and whitespace-only are "unstated", not an unchecked id.
        for blank in ["", "   "] {
            let input = NewBillingSettings {
                country: String::new(),
                vat_id: Some(blank.to_owned()),
                ..valid()
            };
            assert_eq!(
                normalize(&input).map(|s| s.vat_id).unwrap_or_default(),
                None
            );
        }
    }

    #[test]
    fn the_vat_id_is_judged_against_the_issuers_own_country() {
        // An unprefixed id takes the issuer's country and is held to that
        // member state's shape …
        let input = NewBillingSettings {
            country: "NL".to_owned(),
            vat_id: Some("812345678B01".to_owned()),
            ..valid()
        };
        assert_eq!(
            normalize(&input).map(|s| s.vat_id).unwrap_or_default(),
            Some("NL812345678B01".to_owned())
        );
        // … and one that is not that shape is refused.
        let input = NewBillingSettings {
            country: "NL".to_owned(),
            vat_id: Some("81234".to_owned()),
            ..valid()
        };
        assert!(!invalid(normalize(&input)).is_empty());

        // A *prefixed foreign* registration is accepted as written, exactly
        // as it is on a customer ([`crate::vat_id`]): a Dutch company really
        // can be VAT-registered in Germany and invoice under a DE number.
        let input = NewBillingSettings {
            country: "NL".to_owned(),
            vat_id: Some("DE811907980".to_owned()),
            ..valid()
        };
        assert_eq!(
            normalize(&input).map(|s| s.vat_id).unwrap_or_default(),
            Some("DE811907980".to_owned())
        );
    }

    #[test]
    fn the_bank_details_are_held_to_their_own_standard() {
        // Correct length for NL, wrong check digits: the commonest way an
        // IBAN is wrong, and the reason this is checked at all.
        let input = NewBillingSettings {
            iban: Some("NL92ABNA0417164300".to_owned()),
            ..valid()
        };
        assert!(invalid(normalize(&input)).contains("check digits"));

        let input = NewBillingSettings {
            iban: Some("NL91ABNA041716430".to_owned()),
            ..valid()
        };
        assert!(invalid(normalize(&input)).contains("NL IBAN is 18"));

        let input = NewBillingSettings {
            bic: Some("ABNANL2".to_owned()),
            ..valid()
        };
        assert!(invalid(normalize(&input)).contains("BIC"));
    }

    #[test]
    fn an_email_is_an_address_or_nothing() {
        assert_eq!(validate_email("  ").unwrap_or_default(), "");
        assert_eq!(validate_email(" a@b.test ").unwrap_or_default(), "a@b.test");
        for bad in ["a", "a@b", "a b@c.test", "a@@b.test", "@b.test", "a@.test"] {
            assert!(
                matches!(validate_email(bad), Err(StoreError::Validation(_))),
                "expected rejection: {bad:?}"
            );
        }
    }

    #[test]
    fn unstated_settings_are_blank_and_say_so() {
        let blank = BillingSettings::default();
        assert!(!blank.is_stated());
        assert_eq!(blank.effective_account_holder(), "");

        let stated = BillingSettings {
            legal_name: "Alo Werkplaats B.V.".to_owned(),
            updated_at: Some(OffsetDateTime::UNIX_EPOCH),
            ..Default::default()
        };
        assert!(stated.is_stated());
        // No stated holder: the legal name is who the account is in.
        assert_eq!(stated.effective_account_holder(), "Alo Werkplaats B.V.");
        let factored = BillingSettings {
            account_holder: "Alo Trading".to_owned(),
            ..stated
        };
        assert_eq!(factored.effective_account_holder(), "Alo Trading");
    }
}
