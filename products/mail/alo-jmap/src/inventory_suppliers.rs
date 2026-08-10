//! Suppliers HTTP surface (alo Inventory, ADR 0035, wave B5.03) — CRUD over
//! the companies a tenant buys from, on top of [`alo_store::inv_suppliers`].
//!
//! `/inventory` is a **new top-level prefix**: the production Caddyfile needs
//! it added at the next deploy (`docs/design/inventory.md` § Routes), the same
//! note `/billing` carried when it arrived.
//!
//! The conventions are billing's, unchanged, because the rules are the store's
//! and not this module's: authenticated and tenant-scoped through the account
//! door, no validation duplicated from the store, every write answered with the
//! stored record, `PATCH` as a merge onto it, and archiving as its own `POST`
//! so an ordinary edit can never drop a supplier out of the pickers.
//! [`crate::billing::map_store_err`] is used rather than copied — it is a
//! store-error map, not a billing rule.
//!
//! Three refusals worth knowing about, all the store's:
//! a supplier of another tenant is a `404` on every verb (existence is never
//! disclosed), a VAT id whose check digit does not match or an IBAN that fails
//! mod-97 is a `422` naming the rule and **never echoing the value**, and a
//! country that is not two letters is a `422` reported *before* the VAT id it
//! would otherwise be judged against.

use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use serde::Deserialize;
use serde_json::{Value, json};

use alo_store::AccountStore;
use alo_store::InvSupplierId;
use alo_store::inv_suppliers::{NewSupplier, Supplier};

use crate::billing::{absent_or_null, blank_to_none, flag, iso, map_store_err, parse_body};
use crate::error::Problem;
use crate::state::{AppState, authenticate};

/// A supplier as JSON. `paymentTermsDays` is what *they* grant *us* — the
/// mirror of the customer field of the same name — and `leadTimeDays` is the
/// default every offer of theirs inherits.
pub(crate) fn supplier_json(s: &Supplier) -> Value {
    json!({
        "id": s.id.as_str(),
        "name": s.name,
        "addressLine1": s.address_line1,
        "addressLine2": s.address_line2,
        "postalCode": s.postal_code,
        "city": s.city,
        "country": s.country,
        "vatId": s.vat_id,
        "registrationNo": s.registration_no,
        "email": s.email,
        "phone": s.phone,
        "iban": s.iban,
        "currency": s.currency,
        "paymentTermsDays": s.payment_terms_days,
        "leadTimeDays": s.lead_time_days,
        "note": s.note,
        "archived": s.is_archived(),
        "archivedAt": s.archived_at.map(iso),
        "createdBy": s.created_by,
        "createdAt": iso(s.created_at),
        "updatedAt": iso(s.updated_at),
    })
}

/// The stored record as writable input — the base a `PATCH` merges onto.
fn editable(s: &Supplier) -> NewSupplier {
    NewSupplier {
        name: s.name.clone(),
        address_line1: s.address_line1.clone(),
        address_line2: s.address_line2.clone(),
        postal_code: s.postal_code.clone(),
        city: s.city.clone(),
        country: s.country.clone(),
        vat_id: s.vat_id.clone(),
        registration_no: s.registration_no.clone(),
        email: s.email.clone(),
        phone: s.phone.clone(),
        iban: s.iban.clone(),
        currency: s.currency.clone(),
        payment_terms_days: s.payment_terms_days,
        lead_time_days: s.lead_time_days,
        note: s.note.clone(),
    }
}

/// The writable fields of a supplier, every one optional.
///
/// The same body serves `POST` (merged onto [`NewSupplier::default`] — euro,
/// 30-day terms, same-day lead time) and `PATCH` (merged onto the stored
/// record). Unknown fields are ignored so the contract can grow additively.
///
/// The three nullable fields use the absent/`null`/value distinction a plain
/// `Option` cannot express: a VAT id, an email address or an IBAN entered by
/// mistake has to be removable, and "not stated" must not mean "unchanged"
/// there.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct SupplierBody {
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
    #[serde(default)]
    registration_no: Option<String>,
    #[serde(default, deserialize_with = "absent_or_null")]
    email: Option<Option<String>>,
    #[serde(default)]
    phone: Option<String>,
    #[serde(default, deserialize_with = "absent_or_null")]
    iban: Option<Option<String>>,
    #[serde(default)]
    currency: Option<String>,
    #[serde(default)]
    payment_terms_days: Option<i32>,
    #[serde(default)]
    lead_time_days: Option<i32>,
    #[serde(default)]
    note: Option<String>,
}

impl SupplierBody {
    /// Merges the stated fields onto `base`, leaving the rest as they were.
    fn apply(self, base: NewSupplier) -> NewSupplier {
        NewSupplier {
            name: self.name.unwrap_or(base.name),
            address_line1: self.address_line1.unwrap_or(base.address_line1),
            address_line2: self.address_line2.unwrap_or(base.address_line2),
            postal_code: self.postal_code.unwrap_or(base.postal_code),
            city: self.city.unwrap_or(base.city),
            country: self.country.unwrap_or(base.country),
            vat_id: self.vat_id.map_or(base.vat_id, blank_to_none),
            registration_no: self.registration_no.unwrap_or(base.registration_no),
            email: self.email.map_or(base.email, blank_to_none),
            phone: self.phone.unwrap_or(base.phone),
            iban: self.iban.map_or(base.iban, blank_to_none),
            currency: self.currency.unwrap_or(base.currency),
            payment_terms_days: self.payment_terms_days.unwrap_or(base.payment_terms_days),
            lead_time_days: self.lead_time_days.unwrap_or(base.lead_time_days),
            note: self.note.unwrap_or(base.note),
        }
    }
}

/// Loads one of the tenant's suppliers, or fails with the `404` an id from
/// another tenant gets.
pub(crate) async fn load(acc: &AccountStore, id: &InvSupplierId) -> Result<Supplier, Problem> {
    acc.inv_supplier(id)
        .await
        .map_err(map_store_err)?
        .ok_or_else(|| Problem::with(StatusCode::NOT_FOUND, "no such supplier"))
}

/// Query string of the list route.
#[derive(Deserialize)]
pub struct ListQuery {
    /// `includeArchived=1` also returns archived suppliers, sorted after the
    /// active ones. Read through [`flag`], so an unparseable value is simply
    /// off rather than a rejected request.
    #[serde(default, rename = "includeArchived")]
    include_archived: Option<String>,
}

/// `GET /inventory/suppliers[?includeArchived=1]` → `{"suppliers":[…]}` — the
/// list in name order, active ones first.
pub async fn list_suppliers(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(q): Query<ListQuery>,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let suppliers = account
        .acc
        .inv_suppliers(flag(q.include_archived.as_deref()))
        .await
        .map_err(map_store_err)?;
    Ok(Json(json!({
        "suppliers": suppliers.iter().map(supplier_json).collect::<Vec<_>>(),
    })))
}

/// `POST /inventory/suppliers` `{name, country, …}` → `{"supplier":{…}}` —
/// create. `name` and `country` are required; everything else has a default a
/// small business can live with.
pub async fn create_supplier(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: axum::body::Bytes,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let req: SupplierBody = parse_body(&body)?;
    let input = req.apply(NewSupplier::default());
    let id = account
        .acc
        .create_inv_supplier(&input)
        .await
        .map_err(map_store_err)?;
    let supplier = load(&account.acc, &id).await?;
    Ok(Json(json!({ "supplier": supplier_json(&supplier) })))
}

/// `GET /inventory/suppliers/{id}` → `{"supplier":{…}}`. Archived suppliers are
/// readable by id, so an order placed last year can still be explained.
pub async fn get_supplier(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let supplier = load(&account.acc, &InvSupplierId::new(id)).await?;
    Ok(Json(json!({ "supplier": supplier_json(&supplier) })))
}

/// `PATCH /inventory/suppliers/{id}` `{…}` → `{"supplier":{…}}` — merge the
/// stated fields onto the stored record. A changed lead time applies to orders
/// drafted from now on; an order already placed keeps what it copied.
pub async fn update_supplier(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
    body: axum::body::Bytes,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let req: SupplierBody = parse_body(&body)?;
    let id = InvSupplierId::new(id);
    let stored = load(&account.acc, &id).await?;
    let input = req.apply(editable(&stored));
    account
        .acc
        .update_inv_supplier(&id, &input)
        .await
        .map_err(map_store_err)?;
    let supplier = load(&account.acc, &id).await?;
    Ok(Json(json!({ "supplier": supplier_json(&supplier) })))
}

#[derive(Deserialize)]
struct ArchiveBody {
    /// `false` restores. Required when a body is sent; an **empty** body
    /// archives, because the route's name is already the intent.
    archived: bool,
}

/// `POST /inventory/suppliers/{id}/archive` `{"archived":true}` →
/// `{"supplier":{…}}` — stop buying from them, or start again.
///
/// Never a delete: an order that names them has to stay explainable.
/// Idempotent — re-archiving keeps the original time.
pub async fn archive_supplier(
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
    let id = InvSupplierId::new(id);
    account
        .acc
        .set_inv_supplier_archived(&id, req.archived)
        .await
        .map_err(map_store_err)?;
    let supplier = load(&account.acc, &id).await?;
    Ok(Json(json!({ "supplier": supplier_json(&supplier) })))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn body(json: Value) -> SupplierBody {
        serde_json::from_value(json).unwrap_or_else(|e| panic!("body rejected: {e}"))
    }

    fn stored() -> NewSupplier {
        NewSupplier {
            name: "Hoffmann Möbel GmbH".to_owned(),
            city: "Köln".to_owned(),
            country: "DE".to_owned(),
            vat_id: Some("DE811907980".to_owned()),
            email: Some("orders@hoffmann.test".to_owned()),
            iban: Some("NL91ABNA0417164300".to_owned()),
            payment_terms_days: 14,
            lead_time_days: 9,
            ..Default::default()
        }
    }

    #[test]
    fn an_empty_patch_changes_nothing() {
        let merged = body(json!({})).apply(stored());
        assert_eq!(merged.name, "Hoffmann Möbel GmbH");
        assert_eq!(merged.country, "DE");
        assert_eq!(merged.vat_id.as_deref(), Some("DE811907980"));
        assert_eq!(merged.email.as_deref(), Some("orders@hoffmann.test"));
        assert_eq!(merged.iban.as_deref(), Some("NL91ABNA0417164300"));
        assert_eq!(merged.payment_terms_days, 14);
        assert_eq!(merged.lead_time_days, 9);
    }

    #[test]
    fn a_lead_time_edit_leaves_the_rest_of_the_supplier_alone() {
        let merged = body(json!({ "leadTimeDays": 21 })).apply(stored());
        assert_eq!(merged.lead_time_days, 21);
        assert_eq!(merged.name, "Hoffmann Möbel GmbH");
        assert_eq!(merged.payment_terms_days, 14);
    }

    #[test]
    fn zero_is_a_stated_value_not_an_absent_one() {
        // Due on receipt, and a supplier who ships the same day, are both real.
        let merged = body(json!({ "paymentTermsDays": 0, "leadTimeDays": 0 })).apply(stored());
        assert_eq!(merged.payment_terms_days, 0);
        assert_eq!(merged.lead_time_days, 0);
    }

    #[test]
    fn the_three_nullable_fields_can_be_taken_off_again() {
        // `null` clears; a blank string is what a cleared form field sends and
        // means the same thing; absent leaves it alone (above).
        for cleared in [
            json!({"vatId": null, "email": null, "iban": null}),
            json!({"vatId": "", "email": "  ", "iban": ""}),
        ] {
            let merged = body(cleared.clone()).apply(stored());
            assert!(merged.vat_id.is_none(), "VAT id not cleared by {cleared}");
            assert!(merged.email.is_none(), "email not cleared by {cleared}");
            assert!(merged.iban.is_none(), "IBAN not cleared by {cleared}");
        }
        let set = body(json!({"email": "buying@hoffmann.test"})).apply(stored());
        assert_eq!(set.email.as_deref(), Some("buying@hoffmann.test"));
    }

    #[test]
    fn create_starts_from_the_eu_b2b_blanks() {
        let merged =
            body(json!({ "name": "Hoffmann", "country": "de" })).apply(NewSupplier::default());
        assert_eq!(merged.name, "Hoffmann");
        assert_eq!(merged.currency, "EUR");
        assert_eq!(merged.payment_terms_days, 30);
        assert_eq!(merged.lead_time_days, 0);
        assert!(merged.vat_id.is_none() && merged.email.is_none() && merged.iban.is_none());
    }

    #[test]
    fn day_counts_are_integers_on_the_wire() {
        // "14.5 days" is not a payment term; serde's refusal is deliberate.
        assert!(serde_json::from_value::<SupplierBody>(json!({"paymentTermsDays": 14.5})).is_err());
        assert!(serde_json::from_value::<SupplierBody>(json!({"leadTimeDays": "9"})).is_err());
    }
}
