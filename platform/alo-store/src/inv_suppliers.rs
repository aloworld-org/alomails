//! Suppliers — the companies a tenant buys from (alo Inventory, ADR 0035,
//! wave B5.03), reached through the account door like
//! [`crate::billing_customers`].
//!
//! Suppliers are **tenant-wide**: everybody who orders, orders from the same
//! list, so the predicate on every statement is `tenant_id`, taken from the
//! handle and never from request input. A supplier is **archived, never
//! deleted** — a purchase order that names them has to stay explainable, the
//! same rule every master record in this codebase holds.
//!
//! Two decisions worth knowing before reading the code
//! (`docs/design/inventory.md`, "Suppliers"):
//!
//! - **A supplier is not a flagged customer.** The records overlap — name,
//!   address, VAT id, country — and one "company" table with a `supplier` flag
//!   is a design real products ship. It is rejected because the fields diverge
//!   immediately (a customer has payment terms *we* grant; a supplier has lead
//!   times and an IBAN we pay into) and because the failure mode of a wrong
//!   flag is **invoicing a supplier**. Two tables cannot make that mistake.
//! - **A bill does not point here.** [`crate::billing_bills`] carries its
//!   supplier *copied onto the document*, and keeps doing so: a bill must read
//!   exactly as it arrived, years later, whatever has since happened to this
//!   record.
//!
//! Input is normalised once, in [`normalize`], and the same normalisation runs
//! for create and update, so a field can never be stored two different ways
//! depending on which door it came through. The VAT id is held to its member
//! state's shape and check digit by [`crate::vat_id`], the IBAN to ISO 7064
//! mod-97 by [`crate::iban`], and everything the caller can fix is a
//! [`StoreError::Validation`] naming the rule and never echoing the value.

use time::OffsetDateTime;

use crate::account::AccountStore;
use crate::billing_field::{
    bounded, country as validate_country, currency, email as validate_email, payment_terms_days,
    required,
};
use crate::error::{Result, StoreError};
use crate::iban;
use crate::id::InvSupplierId;
use crate::vat_id;

/// The terms and currency rules a supplier shares with every other business
/// record, re-exported so a caller reading about suppliers finds them here.
pub use crate::billing_field::{
    DEFAULT_CURRENCY, DEFAULT_PAYMENT_TERMS_DAYS, PAYMENT_TERMS_MAX_DAYS,
};

/// A supplier name is a legal/display name — generous but bounded, the same
/// bound a customer name carries.
pub const SUPPLIER_NAME_MAX_CHARS: usize = 200;
/// The longest lead time we accept, in days. A year between ordering and
/// arrival is already far beyond anything a small business plans around;
/// beyond it is a typo, and the reorder arithmetic (B5.07) would carry it into
/// a proposal.
pub const LEAD_TIME_MAX_DAYS: i32 = 365;

const ADDRESS_LINE_MAX_CHARS: usize = 200;
const POSTAL_CODE_MAX_CHARS: usize = 20;
const CITY_MAX_CHARS: usize = 120;
const REGISTRATION_NO_MAX_CHARS: usize = 64;
const PHONE_MAX_CHARS: usize = 40;
/// A note is a paragraph about the relationship, not a document.
const NOTE_MAX_CHARS: usize = 2_000;

/// The columns every read of a supplier selects, in `SupplierRow` order.
const SUPPLIER_COLS: &str = "id, name, address_line1, address_line2, postal_code, city, \
     country, vat_id, registration_no, email, phone, iban, currency, payment_terms_days, \
     lead_time_days, note, archived_at, created_by, created_at, updated_at";

/// The writable shape of a supplier, used for both create and update (an
/// update is a full replace — the route layer merges a partial `PATCH` onto
/// the stored record before calling).
///
/// [`Default`] gives the sensible blanks — `EUR`, 30-day terms, same-day lead
/// time, no address — so a caller can write
/// `NewSupplier { name, country, ..Default::default() }`. `name` and `country`
/// have no meaningful default and are always required.
#[derive(Debug, Clone)]
pub struct NewSupplier {
    /// Legal or display name. Required, non-blank.
    pub name: String,
    /// Street address, first line.
    pub address_line1: String,
    /// Street address, second line (suite, department, …).
    pub address_line2: String,
    /// Postal/ZIP code.
    pub postal_code: String,
    /// City / town.
    pub city: String,
    /// ISO 3166-1 alpha-2 country code. Required — it decides which member
    /// state's rules the VAT id is judged by, and whether a purchase from them
    /// is reverse-charged.
    pub country: String,
    /// VAT identification number, or `None` when they have not given one.
    pub vat_id: Option<String>,
    /// Company/registration number as printed on their paper.
    pub registration_no: String,
    /// Where a purchase order is sent, or `None` when unknown.
    pub email: Option<String>,
    /// Telephone number, as the tenant wrote it down.
    pub phone: String,
    /// The account we pay into, or `None` when they have not given one.
    pub iban: Option<String>,
    /// ISO 4217 currency code they quote in.
    pub currency: String,
    /// Days from their invoice date to when we owe them.
    pub payment_terms_days: i32,
    /// Days between ordering and the goods arriving — the default for every
    /// product they sell us.
    pub lead_time_days: i32,
    /// The tenant's own note about the relationship.
    pub note: String,
}

impl Default for NewSupplier {
    fn default() -> Self {
        Self {
            name: String::new(),
            address_line1: String::new(),
            address_line2: String::new(),
            postal_code: String::new(),
            city: String::new(),
            country: String::new(),
            vat_id: None,
            registration_no: String::new(),
            email: None,
            phone: String::new(),
            iban: None,
            currency: DEFAULT_CURRENCY.to_owned(),
            payment_terms_days: DEFAULT_PAYMENT_TERMS_DAYS,
            lead_time_days: 0,
            note: String::new(),
        }
    }
}

/// A stored supplier.
#[derive(Debug, Clone)]
pub struct Supplier {
    /// Opaque id, unique within the tenant.
    pub id: InvSupplierId,
    /// Legal or display name.
    pub name: String,
    /// Street address, first line.
    pub address_line1: String,
    /// Street address, second line.
    pub address_line2: String,
    /// Postal/ZIP code.
    pub postal_code: String,
    /// City / town.
    pub city: String,
    /// ISO 3166-1 alpha-2 country code, uppercase.
    pub country: String,
    /// VAT identification number in canonical prefixed form, `None` when they
    /// have not given one.
    pub vat_id: Option<String>,
    /// Company/registration number.
    pub registration_no: String,
    /// Order email address, `None` when unknown.
    pub email: Option<String>,
    /// Telephone number.
    pub phone: String,
    /// The account we pay into, canonical (no spaces, uppercase).
    pub iban: Option<String>,
    /// ISO 4217 currency code, uppercase.
    pub currency: String,
    /// Days from their invoice date to when we owe them.
    pub payment_terms_days: i32,
    /// Default days between ordering and arrival.
    pub lead_time_days: i32,
    /// The tenant's own note about the relationship.
    pub note: String,
    /// When the supplier was archived; `None` while active.
    pub archived_at: Option<OffsetDateTime>,
    /// The user who created the record.
    pub created_by: String,
    /// Creation time.
    pub created_at: OffsetDateTime,
    /// Last modification time.
    pub updated_at: OffsetDateTime,
}

impl Supplier {
    /// Whether the supplier is archived — hidden from the pickers, still
    /// nameable by every order that already references them.
    pub fn is_archived(&self) -> bool {
        self.archived_at.is_some()
    }
}

/// A validated, normalised supplier ready to be bound into a statement.
#[derive(Debug)]
struct Normalized {
    name: String,
    address_line1: String,
    address_line2: String,
    postal_code: String,
    city: String,
    country: String,
    vat_id: Option<String>,
    registration_no: String,
    email: Option<String>,
    phone: String,
    iban: Option<String>,
    currency: String,
    payment_terms_days: i32,
    lead_time_days: i32,
    note: String,
}

/// Validates a lead time in days: how long after ordering the goods arrive.
/// Zero is valid and common — cash-and-carry, or a supplier who ships the same
/// day.
pub(crate) fn lead_time_days(value: i32) -> Result<i32> {
    if !(0..=LEAD_TIME_MAX_DAYS).contains(&value) {
        return Err(StoreError::Validation(format!(
            "lead time must be between 0 and {LEAD_TIME_MAX_DAYS} days"
        )));
    }
    Ok(value)
}

/// Validates and normalises a whole supplier. Pure — no database, so the rules
/// are unit-tested directly.
///
/// Country is resolved first: it decides which member state's VAT rules the id
/// is held to, so an invalid country is reported before a VAT id that could
/// only ever be judged against it.
fn normalize(input: &NewSupplier) -> Result<Normalized> {
    let country = validate_country(&input.country)?;
    Ok(Normalized {
        name: required("name", &input.name, SUPPLIER_NAME_MAX_CHARS)?,
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
        // Blank is `None`: plenty of small suppliers have no VAT id at all.
        vat_id: match input.vat_id.as_deref() {
            Some(raw) => vat_id::canonicalize(raw, &country)
                .map_err(|error| StoreError::Validation(error.to_string()))?,
            None => None,
        },
        country,
        registration_no: bounded(
            "registration number",
            &input.registration_no,
            REGISTRATION_NO_MAX_CHARS,
        )?,
        email: validate_email(input.email.as_deref())?,
        phone: bounded("phone", &input.phone, PHONE_MAX_CHARS)?,
        // The IBAN's own module owns the rule (country length plus the ISO
        // 7064 mod-97 check); its message names the rule and never the number.
        iban: iban::canonicalize(input.iban.as_deref().unwrap_or_default())
            .map_err(|error| StoreError::Validation(error.to_string()))?,
        currency: currency(&input.currency)?,
        payment_terms_days: payment_terms_days(input.payment_terms_days)?,
        lead_time_days: lead_time_days(input.lead_time_days)?,
        note: bounded("note", &input.note, NOTE_MAX_CHARS)?,
    })
}

impl AccountStore {
    /// Confirms a supplier id is **this tenant's**, so a guessed id from
    /// another tenant is a [`StoreError::NotFound`] rather than a cross-tenant
    /// link.
    ///
    /// The gate on every pointer at a supplier: the product's default supplier
    /// ([`crate::billing_products`]) today, a purchase order's supplier when
    /// B5.05a arrives. `None` passes — not stating a supplier is legitimate.
    ///
    /// # Errors
    /// [`StoreError::NotFound`] when the id is not this tenant's supplier;
    /// [`StoreError::Db`] on failure.
    pub(crate) async fn require_tenant_supplier(&self, id: Option<&InvSupplierId>) -> Result<()> {
        let Some(id) = id else { return Ok(()) };
        let exists: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM inv_suppliers WHERE tenant_id = $1 AND id = $2)",
        )
        .bind(self.tenant.as_str())
        .bind(id.as_str())
        .fetch_one(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        if !exists {
            return Err(StoreError::NotFound);
        }
        Ok(())
    }

    /// Creates an active supplier.
    ///
    /// # Errors
    /// [`StoreError::Validation`] on any field the caller can fix (blank name,
    /// bad country/currency/email/VAT-id/IBAN shape, out-of-range terms or
    /// lead time); [`StoreError::Db`] on failure.
    pub async fn create_inv_supplier(&self, input: &NewSupplier) -> Result<InvSupplierId> {
        let s = normalize(input)?;
        let id = InvSupplierId::generate();
        sqlx::query(
            "INSERT INTO inv_suppliers (tenant_id, id, name, address_line1, address_line2, \
                 postal_code, city, country, vat_id, registration_no, email, phone, iban, \
                 currency, payment_terms_days, lead_time_days, note, created_by) \
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, \
                 $17, $18)",
        )
        .bind(self.tenant.as_str())
        .bind(id.as_str())
        .bind(&s.name)
        .bind(&s.address_line1)
        .bind(&s.address_line2)
        .bind(&s.postal_code)
        .bind(&s.city)
        .bind(&s.country)
        .bind(&s.vat_id)
        .bind(&s.registration_no)
        .bind(&s.email)
        .bind(&s.phone)
        .bind(&s.iban)
        .bind(&s.currency)
        .bind(s.payment_terms_days)
        .bind(s.lead_time_days)
        .bind(&s.note)
        .bind(self.user.as_str())
        .execute(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        Ok(id)
    }

    /// The tenant's suppliers in name order. Archived ones are excluded unless
    /// `include_archived`, and then sort after the active ones.
    ///
    /// # Errors
    /// [`StoreError::Db`] on failure.
    pub async fn inv_suppliers(&self, include_archived: bool) -> Result<Vec<Supplier>> {
        let rows = sqlx::query_as::<_, SupplierRow>(&format!(
            "SELECT {SUPPLIER_COLS} FROM inv_suppliers \
             WHERE tenant_id = $1 AND ($2 OR archived_at IS NULL) \
             ORDER BY (archived_at IS NOT NULL), lower(name), id"
        ))
        .bind(self.tenant.as_str())
        .bind(include_archived)
        .fetch_all(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        Ok(rows.into_iter().map(SupplierRow::into_supplier).collect())
    }

    /// One supplier of the tenant, or `None` — including when the id belongs
    /// to another tenant (indistinguishable by design).
    ///
    /// # Errors
    /// [`StoreError::Db`] on failure.
    pub async fn inv_supplier(&self, id: &InvSupplierId) -> Result<Option<Supplier>> {
        let row = sqlx::query_as::<_, SupplierRow>(&format!(
            "SELECT {SUPPLIER_COLS} FROM inv_suppliers WHERE tenant_id = $1 AND id = $2"
        ))
        .bind(self.tenant.as_str())
        .bind(id.as_str())
        .fetch_optional(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        Ok(row.map(SupplierRow::into_supplier))
    }

    /// Replaces every writable field of a supplier. Archiving is a separate
    /// operation ([`AccountStore::set_inv_supplier_archived`]) so an ordinary
    /// edit can never drop a supplier out of the pickers by accident.
    ///
    /// A changed price or lead time applies to orders drafted **from now on**;
    /// an order already placed keeps what it copied.
    ///
    /// # Errors
    /// [`StoreError::Validation`] as for create; [`StoreError::NotFound`] when
    /// the supplier isn't the tenant's; [`StoreError::Db`] on failure.
    pub async fn update_inv_supplier(&self, id: &InvSupplierId, input: &NewSupplier) -> Result<()> {
        let s = normalize(input)?;
        let done = sqlx::query(
            "UPDATE inv_suppliers SET name = $3, address_line1 = $4, address_line2 = $5, \
                 postal_code = $6, city = $7, country = $8, vat_id = $9, registration_no = $10, \
                 email = $11, phone = $12, iban = $13, currency = $14, payment_terms_days = $15, \
                 lead_time_days = $16, note = $17, updated_at = now() \
             WHERE tenant_id = $1 AND id = $2",
        )
        .bind(self.tenant.as_str())
        .bind(id.as_str())
        .bind(&s.name)
        .bind(&s.address_line1)
        .bind(&s.address_line2)
        .bind(&s.postal_code)
        .bind(&s.city)
        .bind(&s.country)
        .bind(&s.vat_id)
        .bind(&s.registration_no)
        .bind(&s.email)
        .bind(&s.phone)
        .bind(&s.iban)
        .bind(&s.currency)
        .bind(s.payment_terms_days)
        .bind(s.lead_time_days)
        .bind(&s.note)
        .execute(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        if done.rows_affected() == 0 {
            return Err(StoreError::NotFound);
        }
        Ok(())
    }

    /// Archives or restores a supplier. Archiving is the only removal there
    /// is: nothing deletes a supplier, because an order that names them must
    /// stay explainable. Idempotent — archiving an archived supplier keeps the
    /// original archive time.
    ///
    /// # Errors
    /// [`StoreError::NotFound`] when the supplier isn't the tenant's;
    /// [`StoreError::Db`] on failure.
    pub async fn set_inv_supplier_archived(
        &self,
        id: &InvSupplierId,
        archived: bool,
    ) -> Result<()> {
        let done = sqlx::query(
            "UPDATE inv_suppliers \
             SET archived_at = CASE WHEN $3 THEN COALESCE(archived_at, now()) ELSE NULL END, \
                 updated_at = now() \
             WHERE tenant_id = $1 AND id = $2",
        )
        .bind(self.tenant.as_str())
        .bind(id.as_str())
        .bind(archived)
        .execute(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        if done.rows_affected() == 0 {
            return Err(StoreError::NotFound);
        }
        Ok(())
    }
}

// ---- row types --------------------------------------------------------------

#[derive(sqlx::FromRow)]
struct SupplierRow {
    id: String,
    name: String,
    address_line1: String,
    address_line2: String,
    postal_code: String,
    city: String,
    country: String,
    vat_id: Option<String>,
    registration_no: String,
    email: Option<String>,
    phone: String,
    iban: Option<String>,
    currency: String,
    payment_terms_days: i32,
    lead_time_days: i32,
    note: String,
    archived_at: Option<OffsetDateTime>,
    created_by: String,
    created_at: OffsetDateTime,
    updated_at: OffsetDateTime,
}

impl SupplierRow {
    fn into_supplier(self) -> Supplier {
        Supplier {
            id: InvSupplierId::new(self.id),
            name: self.name,
            address_line1: self.address_line1,
            address_line2: self.address_line2,
            postal_code: self.postal_code,
            city: self.city,
            country: self.country,
            vat_id: self.vat_id,
            registration_no: self.registration_no,
            email: self.email,
            phone: self.phone,
            iban: self.iban,
            currency: self.currency,
            payment_terms_days: self.payment_terms_days,
            lead_time_days: self.lead_time_days,
            note: self.note,
            archived_at: self.archived_at,
            created_by: self.created_by,
            created_at: self.created_at,
            updated_at: self.updated_at,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hoffmann() -> NewSupplier {
        NewSupplier {
            name: "Hoffmann Möbel GmbH".to_owned(),
            country: "de".to_owned(),
            ..Default::default()
        }
    }

    fn invalid<T: std::fmt::Debug>(result: Result<T>) -> String {
        match result {
            Err(StoreError::Validation(msg)) => msg,
            other => panic!("expected Validation, got {other:?}"),
        }
    }

    #[test]
    fn defaults_are_the_eu_b2b_blanks() {
        let d = NewSupplier::default();
        assert_eq!(d.currency, DEFAULT_CURRENCY);
        assert_eq!(d.payment_terms_days, DEFAULT_PAYMENT_TERMS_DAYS);
        assert_eq!(
            d.lead_time_days, 0,
            "same-day until somebody says otherwise"
        );
        assert!(d.vat_id.is_none() && d.email.is_none() && d.iban.is_none());
    }

    #[test]
    fn normalize_trims_and_canonicalises_every_stored_form() {
        let input = NewSupplier {
            name: "  Hoffmann Möbel GmbH  ".to_owned(),
            country: "de".to_owned(),
            currency: "eur".to_owned(),
            city: " Köln ".to_owned(),
            vat_id: Some(" de 811.907-980 ".to_owned()),
            email: Some("  orders@hoffmann.test ".to_owned()),
            iban: Some("nl91 abna 0417 1643 00".to_owned()),
            phone: "  +49 221 123456 ".to_owned(),
            ..Default::default()
        };
        let s = normalize(&input).unwrap_or_else(|e| panic!("rejected: {e}"));
        assert_eq!(s.name, "Hoffmann Möbel GmbH");
        assert_eq!(s.country, "DE");
        assert_eq!(s.currency, "EUR");
        assert_eq!(s.city, "Köln");
        // Each canonical form is the one its own module publishes: the VAT id
        // prefixed and separator-free, the IBAN compact and uppercase.
        assert_eq!(s.vat_id.as_deref(), Some("DE811907980"));
        assert_eq!(s.iban.as_deref(), Some("NL91ABNA0417164300"));
        assert_eq!(s.email.as_deref(), Some("orders@hoffmann.test"));
        assert_eq!(s.phone, "+49 221 123456");
    }

    #[test]
    fn name_is_required_and_bounded() {
        for blank in ["", "   ", "\t\n"] {
            let input = NewSupplier {
                name: blank.to_owned(),
                ..hoffmann()
            };
            assert!(invalid(normalize(&input)).contains("name"));
        }
        let input = NewSupplier {
            name: "x".repeat(SUPPLIER_NAME_MAX_CHARS + 1),
            ..hoffmann()
        };
        assert!(invalid(normalize(&input)).contains("at most"));
        let at_bound = NewSupplier {
            name: "x".repeat(SUPPLIER_NAME_MAX_CHARS),
            ..hoffmann()
        };
        assert!(normalize(&at_bound).is_ok());
    }

    #[test]
    fn country_is_required_on_a_supplier() {
        // A supplier must state a country for the same reason a customer must:
        // it decides which member state's VAT rules apply, and whether the
        // purchase is reverse-charged.
        for bad in ["", "D", "DEU", "D1"] {
            let input = NewSupplier {
                country: bad.to_owned(),
                vat_id: None,
                ..hoffmann()
            };
            assert!(
                invalid(normalize(&input)).contains("country"),
                "expected rejection: {bad:?}"
            );
        }
    }

    #[test]
    fn an_invalid_country_is_reported_before_the_vat_id_it_would_judge() {
        let input = NewSupplier {
            country: "Germany".to_owned(),
            vat_id: Some("DE811907980".to_owned()),
            ..hoffmann()
        };
        assert!(invalid(normalize(&input)).contains("country"));
    }

    #[test]
    fn a_vat_id_is_optional_and_country_checked_and_never_echoed() {
        for absent in [None, Some("   ".to_owned())] {
            let input = NewSupplier {
                vat_id: absent,
                ..hoffmann()
            };
            assert_eq!(
                normalize(&input)
                    .unwrap_or_else(|e| panic!("rejected: {e}"))
                    .vat_id,
                None,
                "a small supplier without a VAT id is ordinary"
            );
        }
        // A bare body is judged by the supplier's own country: a German number
        // typed for a Dutch supplier is not a Dutch one.
        let wrong_state = NewSupplier {
            country: "nl".to_owned(),
            vat_id: Some("811907980".to_owned()),
            ..hoffmann()
        };
        assert!(matches!(
            normalize(&wrong_state),
            Err(StoreError::Validation(_))
        ));
        // A foreign registration that names its own country and is valid there
        // is accepted as written — a Dutch company really can invoice a German
        // buyer under its NL number (`vat_id::canonicalize`).
        let foreign = NewSupplier {
            vat_id: Some("NL004495445B01".to_owned()),
            ..hoffmann()
        };
        assert_eq!(
            normalize(&foreign)
                .unwrap_or_else(|e| panic!("rejected: {e}"))
                .vat_id
                .as_deref(),
            Some("NL004495445B01")
        );
        let typo = NewSupplier {
            vat_id: Some("DE811907981".to_owned()),
            ..hoffmann()
        };
        let message = invalid(normalize(&typo));
        assert!(message.contains("check digit"), "{message}");
        assert!(!message.contains("811907981"), "{message}");
    }

    #[test]
    fn an_iban_is_optional_and_mod97_checked_and_never_echoed() {
        let none = NewSupplier {
            iban: Some("  ".to_owned()),
            ..hoffmann()
        };
        assert_eq!(
            normalize(&none)
                .unwrap_or_else(|e| panic!("rejected: {e}"))
                .iban,
            None
        );
        // One digit changed: the mod-97 check is the whole point of an IBAN.
        let typo = NewSupplier {
            iban: Some("NL92ABNA0417164300".to_owned()),
            ..hoffmann()
        };
        let message = invalid(normalize(&typo));
        assert!(message.to_lowercase().contains("iban"), "{message}");
        assert!(!message.contains("ABNA0417164300"), "{message}");
    }

    #[test]
    fn terms_and_lead_time_are_bounded_separately() {
        let terms = NewSupplier {
            payment_terms_days: PAYMENT_TERMS_MAX_DAYS + 1,
            ..hoffmann()
        };
        assert!(invalid(normalize(&terms)).contains("payment terms"));
        for ok in [0, 1, 9, LEAD_TIME_MAX_DAYS] {
            assert_eq!(lead_time_days(ok).unwrap_or(-1), ok);
        }
        for bad in [-1, LEAD_TIME_MAX_DAYS + 1, i32::MIN, i32::MAX] {
            let input = NewSupplier {
                lead_time_days: bad,
                ..hoffmann()
            };
            assert!(
                invalid(normalize(&input)).contains("lead time"),
                "expected rejection: {bad}"
            );
        }
    }

    #[test]
    fn the_free_text_fields_are_bounded() {
        let long = "x".repeat(3_000);
        for input in [
            NewSupplier {
                address_line1: long.clone(),
                ..hoffmann()
            },
            NewSupplier {
                address_line2: long.clone(),
                ..hoffmann()
            },
            NewSupplier {
                postal_code: long.clone(),
                ..hoffmann()
            },
            NewSupplier {
                city: long.clone(),
                ..hoffmann()
            },
            NewSupplier {
                registration_no: long.clone(),
                ..hoffmann()
            },
            NewSupplier {
                phone: long.clone(),
                ..hoffmann()
            },
            NewSupplier {
                note: long.clone(),
                ..hoffmann()
            },
        ] {
            assert!(invalid(normalize(&input)).contains("at most"));
        }
    }
}
