//! Billing customers HTTP surface (alo Billing, ADR 0035, wave B1) — CRUD over
//! the tenant's customer list on top of [`alo_store::billing_customers`].
//!
//! Authenticated and tenant-scoped through the account door: every handler
//! resolves the caller with [`authenticate`] and touches customers only through
//! that handle, so a guessed id from another tenant is the same `404` as an id
//! that was never issued.
//!
//! Three conventions hold across the module:
//!
//! - **No validation lives here.** The store owns every rule (name, country,
//!   currency, email, VAT id, payment terms) and this layer only maps its
//!   answer onto HTTP — otherwise the same field ends up with two definitions
//!   of valid, and the agent (B1.25), which calls the store directly, would
//!   get the other one.
//! - **Every write answers with the stored record**, read back after the
//!   write. The caller sees the canonical form — country and currency
//!   uppercased, the VAT id prefixed and stripped of separators — instead of
//!   the text it sent, and a field name it misspelled is visibly missing from
//!   the response rather than silently dropped.
//! - **`PATCH` is a merge onto the stored record**, then a full replace: an
//!   absent field keeps its value, an explicit `null` (or `""`) clears a
//!   nullable one. It is last-writer-wins — two concurrent edits of different
//!   fields do not merge, and no `ETag`/`If-Match` exists yet. Fine for a
//!   customer record edited by one person at a time; documents that carry
//!   money get the stricter treatment in B1.07.

use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use serde::Deserialize;
use serde_json::{Value, json};

use alo_store::billing_customers::{Customer, NewCustomer};
use alo_store::{AccountStore, BillingCustomerId, ContactId};

use crate::billing::{absent_or_null, blank_to_none, flag, iso, map_store_err, parse_body};
use crate::error::Problem;
use crate::state::{AppState, authenticate};

/// A customer as JSON. `archived` is the flag the UI filters on; `archivedAt`
/// is when it happened, for the record.
pub(crate) fn customer_json(c: &Customer) -> Value {
    json!({
        "id": c.id.as_str(),
        "name": c.name,
        "addressLine1": c.address_line1,
        "addressLine2": c.address_line2,
        "postalCode": c.postal_code,
        "city": c.city,
        "country": c.country,
        "vatId": c.vat_id,
        "email": c.email,
        "paymentTermsDays": c.payment_terms_days,
        "currency": c.currency,
        "contactId": c.contact_id.as_ref().map(ContactId::as_str),
        "archived": c.is_archived(),
        "archivedAt": c.archived_at.map(iso),
        "createdBy": c.created_by,
        "createdAt": iso(c.created_at),
        "updatedAt": iso(c.updated_at),
    })
}

/// The stored record as writable input — the base a `PATCH` merges onto, so a
/// partial edit replays every field the caller did not mention exactly as it
/// stands.
fn editable(c: &Customer) -> NewCustomer {
    NewCustomer {
        name: c.name.clone(),
        address_line1: c.address_line1.clone(),
        address_line2: c.address_line2.clone(),
        postal_code: c.postal_code.clone(),
        city: c.city.clone(),
        country: c.country.clone(),
        vat_id: c.vat_id.clone(),
        email: c.email.clone(),
        payment_terms_days: c.payment_terms_days,
        currency: c.currency.clone(),
        contact_id: c.contact_id.clone(),
    }
}

/// The writable fields of a customer, every one optional.
///
/// The same body serves `POST` (merged onto [`NewCustomer::default`] — the EU
/// B2B blanks) and `PATCH` (merged onto the stored record), so a field can
/// never mean one thing on create and another on edit. Unknown fields are
/// ignored rather than rejected: the surface is a published contract that only
/// ever changes additively, so an older server must tolerate a newer client's
/// field. The response carries the stored record, which is where a caller sees
/// that a misspelled field did nothing.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CustomerBody {
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    address_line1: Option<String>,
    #[serde(default)]
    address_line2: Option<String>,
    #[serde(default)]
    postal_code: Option<String>,
    #[serde(default)]
    city: Option<String>,
    #[serde(default)]
    country: Option<String>,
    #[serde(default, deserialize_with = "absent_or_null")]
    vat_id: Option<Option<String>>,
    #[serde(default, deserialize_with = "absent_or_null")]
    email: Option<Option<String>>,
    #[serde(default)]
    payment_terms_days: Option<i32>,
    #[serde(default)]
    currency: Option<String>,
    #[serde(default, deserialize_with = "absent_or_null")]
    contact_id: Option<Option<String>>,
}

impl CustomerBody {
    /// Merges the stated fields onto `base`, leaving the rest as they were.
    fn apply(self, base: NewCustomer) -> NewCustomer {
        NewCustomer {
            name: self.name.unwrap_or(base.name),
            address_line1: self.address_line1.unwrap_or(base.address_line1),
            address_line2: self.address_line2.unwrap_or(base.address_line2),
            postal_code: self.postal_code.unwrap_or(base.postal_code),
            city: self.city.unwrap_or(base.city),
            country: self.country.unwrap_or(base.country),
            vat_id: self.vat_id.map_or(base.vat_id, blank_to_none),
            email: self.email.map_or(base.email, blank_to_none),
            payment_terms_days: self.payment_terms_days.unwrap_or(base.payment_terms_days),
            currency: self.currency.unwrap_or(base.currency),
            contact_id: self
                .contact_id
                .map_or(base.contact_id, |v| blank_to_none(v).map(ContactId::new)),
        }
    }
}

/// Loads one of the tenant's customers, or fails with the `404` an id from
/// another tenant gets.
async fn load(acc: &AccountStore, id: &BillingCustomerId) -> Result<Customer, Problem> {
    acc.billing_customer(id)
        .await
        .map_err(map_store_err)?
        .ok_or_else(|| Problem::with(StatusCode::NOT_FOUND, "no such customer"))
}

/// Query string of the list route.
#[derive(Deserialize)]
pub struct ListQuery {
    /// `includeArchived=1` also returns archived customers, sorted after the
    /// active ones. Read through [`flag`], so an unparseable value is simply
    /// off rather than a rejected request.
    #[serde(default, rename = "includeArchived")]
    include_archived: Option<String>,
}

/// `GET /billing/customers[?includeArchived=1]` → `{"customers":[…]}` — the
/// tenant's customers in name order, active ones first.
pub async fn list_customers(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(q): Query<ListQuery>,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let customers =
        crate::billing_intents::customers(&account, flag(q.include_archived.as_deref())).await?;
    Ok(Json(json!({
        "customers": customers.iter().map(customer_json).collect::<Vec<_>>(),
    })))
}

/// `POST /billing/customers` `{name, country, …}` → `{"customer":{…}}` —
/// create. `name` and `country` are required by the store; everything else
/// falls back to the EU B2B defaults (30-day terms, EUR, no VAT id).
pub async fn create_customer(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: axum::body::Bytes,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let req: CustomerBody = parse_body(&body)?;
    let input = req.apply(NewCustomer::default());
    let id = account
        .acc
        .create_billing_customer(&input)
        .await
        .map_err(map_store_err)?;
    let customer = load(&account.acc, &id).await?;
    Ok(Json(json!({ "customer": customer_json(&customer) })))
}

/// `GET /billing/customers/{id}` → `{"customer":{…}}`. Archived customers are
/// readable by id — an issued invoice must always be able to name its party.
pub async fn get_customer(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let customer = load(&account.acc, &BillingCustomerId::new(id)).await?;
    Ok(Json(json!({ "customer": customer_json(&customer) })))
}

/// `PATCH /billing/customers/{id}` `{…}` → `{"customer":{…}}` — merge the
/// stated fields onto the stored record. Archiving is deliberately not one of
/// them (see [`archive_customer`]).
pub async fn update_customer(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
    body: axum::body::Bytes,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let req: CustomerBody = parse_body(&body)?;
    let id = BillingCustomerId::new(id);
    let stored = load(&account.acc, &id).await?;
    let input = req.apply(editable(&stored));
    account
        .acc
        .update_billing_customer(&id, &input)
        .await
        .map_err(map_store_err)?;
    let customer = load(&account.acc, &id).await?;
    Ok(Json(json!({ "customer": customer_json(&customer) })))
}

#[derive(Deserialize)]
struct ArchiveBody {
    /// `false` restores. Required when a body is sent — a request that states
    /// the field must state it correctly; an **empty** body archives, because
    /// the route's name is already the intent.
    archived: bool,
}

/// `POST /billing/customers/{id}/archive` `{"archived":true}` →
/// `{"customer":{…}}` — archive or restore.
///
/// A customer is never deleted: an issued invoice must always be able to name
/// the party it was raised for. Archiving only hides the record from the
/// pickers, and it is idempotent — re-archiving keeps the original time.
/// Separate from `PATCH` on purpose, so an ordinary edit can never drop a
/// customer out of the pickers by accident.
pub async fn archive_customer(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
    body: axum::body::Bytes,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let req: ArchiveBody = parse_body(if body.is_empty() {
        br#"{"archived":true}"#
    } else {
        &body
    })?;
    let id = BillingCustomerId::new(id);
    account
        .acc
        .set_billing_customer_archived(&id, req.archived)
        .await
        .map_err(map_store_err)?;
    let customer = load(&account.acc, &id).await?;
    Ok(Json(json!({ "customer": customer_json(&customer) })))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn body(json: Value) -> CustomerBody {
        serde_json::from_value(json).unwrap_or_else(|e| panic!("body rejected: {e}"))
    }

    fn stored() -> NewCustomer {
        NewCustomer {
            name: "Acme GmbH".to_owned(),
            city: "Berlin".to_owned(),
            country: "DE".to_owned(),
            vat_id: Some("DE811907980".to_owned()),
            email: Some("billing@acme.test".to_owned()),
            payment_terms_days: 14,
            currency: "EUR".to_owned(),
            contact_id: Some(ContactId::new("c-1".to_owned())),
            ..Default::default()
        }
    }

    #[test]
    fn an_empty_patch_changes_nothing() {
        let merged = body(json!({})).apply(stored());
        let base = stored();
        assert_eq!(merged.name, base.name);
        assert_eq!(merged.city, base.city);
        assert_eq!(merged.country, base.country);
        assert_eq!(merged.vat_id, base.vat_id);
        assert_eq!(merged.email, base.email);
        assert_eq!(merged.payment_terms_days, base.payment_terms_days);
        assert_eq!(merged.currency, base.currency);
        assert_eq!(
            merged.contact_id.map(|c| c.as_str().to_owned()),
            Some("c-1".to_owned())
        );
    }

    #[test]
    fn a_stated_field_replaces_and_leaves_its_neighbours_alone() {
        let merged = body(json!({ "city": "Hamburg", "paymentTermsDays": 30 })).apply(stored());
        assert_eq!(merged.city, "Hamburg");
        assert_eq!(merged.payment_terms_days, 30);
        assert_eq!(merged.name, "Acme GmbH");
        assert_eq!(merged.vat_id.as_deref(), Some("DE811907980"));
    }

    #[test]
    fn null_and_blank_both_clear_a_nullable_field() {
        for clearing in [json!(null), json!(""), json!("   ")] {
            let merged = body(json!({
                "vatId": clearing, "email": clearing, "contactId": clearing,
            }))
            .apply(stored());
            assert_eq!(merged.vat_id, None, "vatId not cleared by {clearing}");
            assert_eq!(merged.email, None, "email not cleared by {clearing}");
            assert!(merged.contact_id.is_none(), "contactId not cleared");
        }
    }

    #[test]
    fn create_starts_from_the_eu_b2b_blanks() {
        let merged =
            body(json!({ "name": "Acme GmbH", "country": "de" })).apply(NewCustomer::default());
        assert_eq!(merged.name, "Acme GmbH");
        assert_eq!(merged.country, "de");
        assert_eq!(
            merged.payment_terms_days,
            alo_store::billing_customers::DEFAULT_PAYMENT_TERMS_DAYS
        );
        assert_eq!(
            merged.currency,
            alo_store::billing_customers::DEFAULT_CURRENCY
        );
        assert_eq!(merged.vat_id, None);
    }

    #[test]
    fn a_wrongly_typed_field_is_refused_rather_than_coerced() {
        // Money and day counts are integers; "30" is a string and a client
        // that sends one has a bug we should not paper over.
        assert!(serde_json::from_value::<CustomerBody>(json!({"paymentTermsDays": "30"})).is_err());
        assert!(serde_json::from_value::<CustomerBody>(json!({"name": 7})).is_err());
    }

    #[test]
    fn an_unknown_field_is_ignored_so_the_contract_can_grow() {
        let merged = body(json!({ "city": "Hamburg", "vatID": "DE811907980" })).apply(stored());
        assert_eq!(merged.city, "Hamburg");
        // The misspelling did nothing — and the response the caller gets back
        // is the stored record, where that is visible.
        assert_eq!(merged.vat_id.as_deref(), Some("DE811907980"));
    }
}
