//! Billing customers — the companies and people a tenant invoices (alo
//! Billing, ADR 0035, wave B1), reached through the account door like
//! [`crate::sites`] and [`crate::tasks`].
//!
//! Customers are **tenant-wide**: every user of the tenant bills the same
//! list, so the predicate on every statement is `tenant_id`, taken from the
//! handle and never from request input. A customer is **archived, never
//! deleted** — an issued invoice must always be able to name the party it was
//! raised for (`docs/design/billing.md`).
//!
//! Input is normalised once, in [`normalize`], and the same normalisation runs
//! for create and update, so a field can never be stored two different ways
//! depending on which door it came through. The VAT id is held to its member
//! state's published shape and check digit by [`crate::vat_id`] and stored in
//! the canonical prefixed form (`DE811907980`) that e-invoicing wants.
//! Everything the caller can fix is a
//! [`StoreError::Validation`] naming the rule; a link to an address-book
//! contact that isn't the tenant's is a [`StoreError::NotFound`], the same
//! answer as a contact that does not exist at all.

use std::collections::HashMap;

use time::OffsetDateTime;

use crate::account::AccountStore;
use crate::billing_field::{
    bounded, country as validate_country, currency, email as validate_email, payment_terms_days,
    required,
};
use crate::error::{Result, StoreError};
use crate::id::{BillingCustomerId, ContactId};
use crate::vat_id;

/// The terms and currency rules a customer shares with every other billing
/// record, re-exported so a caller reading about customers finds them here.
pub use crate::billing_field::{
    DEFAULT_CURRENCY, DEFAULT_PAYMENT_TERMS_DAYS, PAYMENT_TERMS_MAX_DAYS,
};

/// A customer name is a legal/display name — generous but bounded.
pub const CUSTOMER_NAME_MAX_CHARS: usize = 200;

const ADDRESS_LINE_MAX_CHARS: usize = 200;
const POSTAL_CODE_MAX_CHARS: usize = 20;
const CITY_MAX_CHARS: usize = 120;

/// The columns every read of a customer selects, in `CustomerRow` order.
const CUSTOMER_COLS: &str = "id, name, address_line1, address_line2, postal_code, city, \
     country, vat_id, email, payment_terms_days, currency, contact_id, archived_at, \
     created_by, created_at, updated_at";

/// The writable shape of a customer, used for both create and update (an
/// update is a full replace — the route layer merges a partial `PATCH` onto
/// the stored record before calling).
///
/// [`Default`] gives the sensible blanks — `EUR`, 30-day terms, no address —
/// so a caller can write `NewCustomer { name, country, ..Default::default() }`.
/// `name` and `country` have no meaningful default and are always required.
#[derive(Debug, Clone)]
pub struct NewCustomer {
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
    /// ISO 3166-1 alpha-2 country code. Required — it drives VAT treatment.
    pub country: String,
    /// VAT identification number, or `None` for a B2C customer.
    pub vat_id: Option<String>,
    /// Where invoices are sent, or `None` when unknown.
    pub email: Option<String>,
    /// Days from issue to due date.
    pub payment_terms_days: i32,
    /// ISO 4217 currency code for documents raised for this customer.
    pub currency: String,
    /// Optional link to one of the tenant's address-book contacts.
    pub contact_id: Option<ContactId>,
}

impl Default for NewCustomer {
    fn default() -> Self {
        Self {
            name: String::new(),
            address_line1: String::new(),
            address_line2: String::new(),
            postal_code: String::new(),
            city: String::new(),
            country: String::new(),
            vat_id: None,
            email: None,
            payment_terms_days: DEFAULT_PAYMENT_TERMS_DAYS,
            currency: DEFAULT_CURRENCY.to_owned(),
            contact_id: None,
        }
    }
}

/// A stored customer.
#[derive(Debug, Clone)]
pub struct Customer {
    /// Opaque id, unique within the tenant.
    pub id: BillingCustomerId,
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
    /// VAT identification number, `None` for B2C.
    pub vat_id: Option<String>,
    /// Invoice email address, `None` when unknown.
    pub email: Option<String>,
    /// Days from issue to due date.
    pub payment_terms_days: i32,
    /// ISO 4217 currency code, uppercase.
    pub currency: String,
    /// Linked address-book contact, if any.
    pub contact_id: Option<ContactId>,
    /// When the customer was archived; `None` while active.
    pub archived_at: Option<OffsetDateTime>,
    /// The user who created the record.
    pub created_by: String,
    /// Creation time.
    pub created_at: OffsetDateTime,
    /// Last modification time.
    pub updated_at: OffsetDateTime,
}

impl Customer {
    /// Whether the customer is archived (hidden from pickers, still nameable
    /// by every document that already references it).
    pub fn is_archived(&self) -> bool {
        self.archived_at.is_some()
    }
}

/// A validated, normalised customer ready to be bound into a statement.
#[derive(Debug)]
struct Normalized {
    name: String,
    address_line1: String,
    address_line2: String,
    postal_code: String,
    city: String,
    country: String,
    vat_id: Option<String>,
    email: Option<String>,
    payment_terms_days: i32,
    currency: String,
    contact_id: Option<String>,
}

/// Validates and canonicalises an optional VAT id for a customer in
/// `country`, delegating every rule to [`crate::vat_id`]: separators and case
/// are presentation, the stored form always carries its country prefix, and
/// the member state's shape (plus its check digit, where one is published) has
/// to hold. Blank is `None` — B2C customers have no VAT id and that is not an
/// error.
fn normalize_vat_id(vat_id: Option<&str>, country: &str) -> Result<Option<String>> {
    let Some(raw) = vat_id else { return Ok(None) };
    vat_id::canonicalize(raw, country).map_err(|error| StoreError::Validation(error.to_string()))
}

/// Validates and normalises a whole customer. Pure — no database, so the
/// rules are unit-tested directly.
///
/// Country is resolved first: it decides which member state's VAT rules the
/// id is held to, so an invalid country is reported before a VAT id that
/// could only ever be judged against it.
fn normalize(input: &NewCustomer) -> Result<Normalized> {
    let country = validate_country(&input.country)?;
    Ok(Normalized {
        name: required("name", &input.name, CUSTOMER_NAME_MAX_CHARS)?,
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
        email: validate_email(input.email.as_deref())?,
        payment_terms_days: payment_terms_days(input.payment_terms_days)?,
        currency: currency(&input.currency)?,
        contact_id: input.contact_id.as_ref().map(|c| c.as_str().to_owned()),
    })
}

/// Translates a violation of the contact foreign key into the same
/// [`StoreError::NotFound`] an unknown contact id gets. This is the race
/// window: the contact existed when we checked and was deleted before the
/// write landed. Anything else passes through the standard mapping.
fn map_contact_fk(error: sqlx::Error) -> StoreError {
    if let sqlx::Error::Database(ref db) = error
        && db.constraint() == Some("billing_customers_contact_fk")
    {
        return StoreError::NotFound;
    }
    error.into()
}

/// Reads one customer of `tenant`, or `None` — **the one place a customer is
/// read by id**.
///
/// Takes any executor so the same read serves the ordinary pool call
/// ([`AccountStore::billing_customer`]) and a read inside a transaction that
/// must not see a customer archived a moment later — which is what the
/// timesheet handoff ([`crate::time_invoice`]) needs when it resolves the
/// document's header and its lines in one transaction.
///
/// # Errors
/// [`StoreError::Db`] on failure.
pub(crate) async fn customer_read<'e, E>(
    executor: E,
    tenant: &str,
    id: &BillingCustomerId,
) -> Result<Option<Customer>>
where
    E: sqlx::Executor<'e, Database = sqlx::Postgres>,
{
    let row = sqlx::query_as::<_, CustomerRow>(&format!(
        "SELECT {CUSTOMER_COLS} FROM billing_customers WHERE tenant_id = $1 AND id = $2"
    ))
    .bind(tenant)
    .bind(id.as_str())
    .fetch_optional(executor)
    .await
    .map_err(StoreError::Db)?;
    Ok(row.map(CustomerRow::into_customer))
}

impl AccountStore {
    /// Confirms a linked contact is **this tenant's**, so a guessed id from
    /// another tenant is a `NotFound` rather than a cross-tenant link.
    ///
    /// Shared with [`crate::crm_deals`], which carries the same optional
    /// address-book pointer under the same asymmetry (contacts are per user, a
    /// deal is tenant-wide), so both doors hold it to one rule.
    pub(crate) async fn require_tenant_contact(&self, contact_id: Option<&String>) -> Result<()> {
        let Some(contact_id) = contact_id else {
            return Ok(());
        };
        let exists: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM contacts WHERE tenant_id = $1 AND id = $2)",
        )
        .bind(self.tenant.as_str())
        .bind(contact_id)
        .fetch_one(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        if !exists {
            return Err(StoreError::NotFound);
        }
        Ok(())
    }

    /// Creates an active customer.
    ///
    /// # Errors
    /// [`StoreError::Validation`] on any field the caller can fix (blank name,
    /// bad country/currency/email/VAT-id shape, out-of-range terms);
    /// [`StoreError::NotFound`] when `contact_id` is not this tenant's
    /// contact; [`StoreError::Db`] on failure.
    pub async fn create_billing_customer(&self, input: &NewCustomer) -> Result<BillingCustomerId> {
        let c = normalize(input)?;
        self.require_tenant_contact(c.contact_id.as_ref()).await?;
        let id = BillingCustomerId::generate();
        sqlx::query(
            "INSERT INTO billing_customers (tenant_id, id, name, address_line1, address_line2, \
                 postal_code, city, country, vat_id, email, payment_terms_days, currency, \
                 contact_id, created_by) \
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14)",
        )
        .bind(self.tenant.as_str())
        .bind(id.as_str())
        .bind(&c.name)
        .bind(&c.address_line1)
        .bind(&c.address_line2)
        .bind(&c.postal_code)
        .bind(&c.city)
        .bind(&c.country)
        .bind(&c.vat_id)
        .bind(&c.email)
        .bind(c.payment_terms_days)
        .bind(&c.currency)
        .bind(&c.contact_id)
        .bind(self.user.as_str())
        .execute(&self.pool)
        .await
        .map_err(map_contact_fk)?;
        Ok(id)
    }

    /// The tenant's customers in name order. Archived ones are excluded unless
    /// `include_archived`, and then sort after the active ones.
    ///
    /// # Errors
    /// [`StoreError::Db`] on failure.
    pub async fn billing_customers(&self, include_archived: bool) -> Result<Vec<Customer>> {
        let rows = sqlx::query_as::<_, CustomerRow>(&format!(
            "SELECT {CUSTOMER_COLS} FROM billing_customers \
             WHERE tenant_id = $1 AND ($2 OR archived_at IS NULL) \
             ORDER BY (archived_at IS NOT NULL), lower(name), id"
        ))
        .bind(self.tenant.as_str())
        .bind(include_archived)
        .fetch_all(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        Ok(rows.into_iter().map(CustomerRow::into_customer).collect())
    }

    /// The names of the tenant's customers named by `ids`, keyed by id.
    ///
    /// The name and nothing else, in one statement rather than one read per
    /// customer: the reconciliation suggestions ([`crate::bank_suggest`]) need a
    /// name per open document and would otherwise either read every customer the
    /// tenant has or read one per invoice. An id that is not this tenant's is
    /// simply absent from the answer, exactly as it is from every other read on
    /// this door.
    ///
    /// # Errors
    /// [`StoreError::Db`] on failure.
    pub async fn billing_customer_names(
        &self,
        ids: &[BillingCustomerId],
    ) -> Result<HashMap<String, String>> {
        if ids.is_empty() {
            return Ok(HashMap::new());
        }
        let wanted: Vec<String> = ids.iter().map(|id| id.as_str().to_owned()).collect();
        let rows: Vec<(String, String)> = sqlx::query_as(
            "SELECT id, name FROM billing_customers \
             WHERE tenant_id = $1 AND id = ANY($2::text[])",
        )
        .bind(self.tenant.as_str())
        .bind(&wanted)
        .fetch_all(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        Ok(rows.into_iter().collect())
    }

    /// One customer of the tenant, or `None` — including when the id belongs
    /// to another tenant (indistinguishable by design).
    ///
    /// # Errors
    /// [`StoreError::Db`] on failure.
    pub async fn billing_customer(&self, id: &BillingCustomerId) -> Result<Option<Customer>> {
        customer_read(&self.pool, self.tenant.as_str(), id).await
    }

    /// Replaces every writable field of a customer. Archiving is a separate
    /// operation ([`AccountStore::set_billing_customer_archived`]) so an
    /// ordinary edit can never flip a customer out of the pickers by accident.
    ///
    /// # Errors
    /// [`StoreError::Validation`] as for create; [`StoreError::NotFound`] when
    /// the customer isn't the tenant's, or `contact_id` isn't its contact;
    /// [`StoreError::Db`] on failure.
    pub async fn update_billing_customer(
        &self,
        id: &BillingCustomerId,
        input: &NewCustomer,
    ) -> Result<()> {
        let c = normalize(input)?;
        self.require_tenant_contact(c.contact_id.as_ref()).await?;
        let done = sqlx::query(
            "UPDATE billing_customers SET name = $3, address_line1 = $4, address_line2 = $5, \
                 postal_code = $6, city = $7, country = $8, vat_id = $9, email = $10, \
                 payment_terms_days = $11, currency = $12, contact_id = $13, updated_at = now() \
             WHERE tenant_id = $1 AND id = $2",
        )
        .bind(self.tenant.as_str())
        .bind(id.as_str())
        .bind(&c.name)
        .bind(&c.address_line1)
        .bind(&c.address_line2)
        .bind(&c.postal_code)
        .bind(&c.city)
        .bind(&c.country)
        .bind(&c.vat_id)
        .bind(&c.email)
        .bind(c.payment_terms_days)
        .bind(&c.currency)
        .bind(&c.contact_id)
        .execute(&self.pool)
        .await
        .map_err(map_contact_fk)?;
        if done.rows_affected() == 0 {
            return Err(StoreError::NotFound);
        }
        Ok(())
    }

    /// Archives or restores a customer. Archiving is the only removal there
    /// is: nothing deletes a customer, because an issued invoice must always
    /// be able to name the party it was raised for. Idempotent — archiving an
    /// archived customer keeps the original archive time.
    ///
    /// # Errors
    /// [`StoreError::NotFound`] when the customer isn't the tenant's;
    /// [`StoreError::Db`] on failure.
    pub async fn set_billing_customer_archived(
        &self,
        id: &BillingCustomerId,
        archived: bool,
    ) -> Result<()> {
        let done = sqlx::query(
            "UPDATE billing_customers \
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
struct CustomerRow {
    id: String,
    name: String,
    address_line1: String,
    address_line2: String,
    postal_code: String,
    city: String,
    country: String,
    vat_id: Option<String>,
    email: Option<String>,
    payment_terms_days: i32,
    currency: String,
    contact_id: Option<String>,
    archived_at: Option<OffsetDateTime>,
    created_by: String,
    created_at: OffsetDateTime,
    updated_at: OffsetDateTime,
}

impl CustomerRow {
    fn into_customer(self) -> Customer {
        Customer {
            id: BillingCustomerId::new(self.id),
            name: self.name,
            address_line1: self.address_line1,
            address_line2: self.address_line2,
            postal_code: self.postal_code,
            city: self.city,
            country: self.country,
            vat_id: self.vat_id,
            email: self.email,
            payment_terms_days: self.payment_terms_days,
            currency: self.currency,
            contact_id: self.contact_id.map(ContactId::new),
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

    fn valid() -> NewCustomer {
        NewCustomer {
            name: "Acme GmbH".to_owned(),
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
        let d = NewCustomer::default();
        assert_eq!(d.payment_terms_days, DEFAULT_PAYMENT_TERMS_DAYS);
        assert_eq!(d.currency, DEFAULT_CURRENCY);
        assert!(d.vat_id.is_none() && d.email.is_none() && d.contact_id.is_none());
    }

    #[test]
    fn normalize_trims_and_uppercases() {
        let input = NewCustomer {
            name: "  Acme GmbH  ".to_owned(),
            country: "de".to_owned(),
            currency: "eur".to_owned(),
            city: " Berlin ".to_owned(),
            vat_id: Some(" de 811.907-980 ".to_owned()),
            email: Some("  billing@acme.test ".to_owned()),
            ..Default::default()
        };
        let c = normalize(&input).unwrap_or_else(|e| panic!("rejected: {e}"));
        assert_eq!(c.name, "Acme GmbH");
        assert_eq!(c.country, "DE");
        assert_eq!(c.currency, "EUR");
        assert_eq!(c.city, "Berlin");
        assert_eq!(c.vat_id.as_deref(), Some("DE811907980"));
        assert_eq!(c.email.as_deref(), Some("billing@acme.test"));
    }

    #[test]
    fn name_is_required_and_bounded() {
        for blank in ["", "   ", "\t\n"] {
            let input = NewCustomer {
                name: blank.to_owned(),
                ..valid()
            };
            assert!(invalid(normalize(&input)).contains("name"));
        }
        let input = NewCustomer {
            name: "x".repeat(CUSTOMER_NAME_MAX_CHARS + 1),
            ..valid()
        };
        assert!(invalid(normalize(&input)).contains("at most"));
        // Exactly at the bound is fine.
        let input = NewCustomer {
            name: "x".repeat(CUSTOMER_NAME_MAX_CHARS),
            ..valid()
        };
        assert!(normalize(&input).is_ok());
    }

    #[test]
    fn country_is_required_on_a_customer() {
        // The shape rule itself is `billing_field::country`'s (and is tested
        // there); what is this record's own is that a customer must state a
        // country at all — it decides their VAT treatment.
        for bad in ["", "D", "DEU", "D1"] {
            let input = NewCustomer {
                country: bad.to_owned(),
                vat_id: None,
                ..valid()
            };
            assert!(
                invalid(normalize(&input)).contains("country"),
                "expected rejection: {bad:?}"
            );
        }
        assert_eq!(validate_country("be").unwrap_or_default(), "BE");
    }

    #[test]
    fn email_is_optional_but_well_formed_when_present() {
        // The rule itself is `billing_field::email`'s (and is tested there);
        // what is pinned here is that a customer actually goes through it —
        // blank is the printed-invoice customer, malformed is a refusal.
        let blank = NewCustomer {
            email: Some("   ".to_owned()),
            ..valid()
        };
        assert_eq!(
            normalize(&blank)
                .unwrap_or_else(|e| panic!("rejected: {e}"))
                .email,
            None
        );
        let good = NewCustomer {
            email: Some("  billing@acme.test ".to_owned()),
            ..valid()
        };
        assert_eq!(
            normalize(&good)
                .unwrap_or_else(|e| panic!("rejected: {e}"))
                .email
                .as_deref(),
            Some("billing@acme.test")
        );
        let bad = NewCustomer {
            email: Some("two@at@signs.test".to_owned()),
            ..valid()
        };
        assert!(invalid(normalize(&bad)).contains("email"));
    }

    #[test]
    fn vat_id_is_optional_canonical_and_country_checked() {
        // No VAT id at all, and a blank one, are both the B2C customer.
        assert_eq!(normalize_vat_id(None, "DE").unwrap_or_default(), None);
        assert_eq!(normalize_vat_id(Some("  "), "DE").unwrap_or_default(), None);
        // Stored canonical: prefixed, uppercase, no separators.
        assert_eq!(
            normalize_vat_id(Some("nl 0044.95445-B01"), "NL").unwrap_or_default(),
            Some("NL004495445B01".to_owned())
        );
        // The country decides the rules: a German id is not a Dutch one.
        assert!(matches!(
            normalize_vat_id(Some("811907980"), "NL"),
            Err(StoreError::Validation(_))
        ));
        for bad in ["DE811907980!", "DE811907981", "DE81190798"] {
            assert!(
                matches!(
                    normalize_vat_id(Some(bad), "DE"),
                    Err(StoreError::Validation(_))
                ),
                "expected rejection: {bad:?}"
            );
        }
        // The failing rule reaches the caller, but never the id itself.
        let message = invalid(normalize_vat_id(Some("DE811907981"), "DE"));
        assert!(message.contains("check digit"), "{message}");
        assert!(!message.contains("811907981"), "{message}");
    }

    #[test]
    fn an_invalid_country_is_reported_before_the_vat_id_it_would_judge() {
        let input = NewCustomer {
            country: "Germany".to_owned(),
            vat_id: Some("DE811907980".to_owned()),
            ..valid()
        };
        assert!(invalid(normalize(&input)).contains("country"));
    }

    #[test]
    fn out_of_range_payment_terms_are_refused_on_the_customer_path() {
        // The rule itself lives in `billing_field`; this pins that a customer
        // actually goes through it.
        let input = NewCustomer {
            payment_terms_days: PAYMENT_TERMS_MAX_DAYS + 1,
            ..valid()
        };
        assert!(invalid(normalize(&input)).contains("payment terms"));
    }

    #[test]
    fn address_fields_are_bounded() {
        let long = "x".repeat(1000);
        for input in [
            NewCustomer {
                address_line1: long.clone(),
                ..valid()
            },
            NewCustomer {
                address_line2: long.clone(),
                ..valid()
            },
            NewCustomer {
                postal_code: long.clone(),
                ..valid()
            },
            NewCustomer {
                city: long.clone(),
                ..valid()
            },
        ] {
            assert!(invalid(normalize(&input)).contains("at most"));
        }
    }
}
