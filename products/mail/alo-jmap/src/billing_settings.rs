//! The issuer identity behind every billing document, over HTTP (alo Billing,
//! ADR 0035, wave B1) — read and save the tenant's own name, numbers, address
//! and bank details on top of [`alo_store::billing_settings`].
//!
//! The same conventions as [`crate::billing_customers`]: authenticated and
//! tenant-scoped through the account door, no validation duplicated from the
//! store, the write answered with the stored record, and a partial body as a
//! merge onto it.
//!
//! What is different is that there is **exactly one** of these per tenant, so
//! the resource has no id and no list:
//!
//! - `GET` **never answers `404`.** A tenant that has never saved reads the
//!   blanks with `stated: false`. There is no "not configured" error for a
//!   record with one row, and a print view must not have to ask.
//! - The write is a **`PATCH`**, not a `PUT`, because it behaves like one:
//!   absent fields keep their stored value and `null` clears a nullable one,
//!   which is the same merge every other billing record's edit performs. A
//!   `PUT` would promise a whole-document replace and quietly blank whatever
//!   an older client did not know to send. Last writer wins, as everywhere in
//!   billing — there is no `ETag` yet.

use axum::Json;
use axum::extract::State;
use axum::http::HeaderMap;
use serde::Deserialize;
use serde_json::{Value, json};

use alo_store::billing_settings::{BillingSettings, NewBillingSettings};

use crate::billing::{absent_or_null, blank_to_none, iso, map_store_err, parse_body};
use crate::error::Problem;
use crate::state::{AppState, authenticate};

/// The issuer identity as JSON.
///
/// `stated` is the flag that matters to a caller: `false` means this tenant
/// has never saved, so every blank below is "not yet said" rather than
/// "deliberately empty". `updatedBy`/`updatedAt` are `null` in that state.
fn settings_json(s: &BillingSettings) -> Value {
    json!({
        "stated": s.is_stated(),
        "legalName": s.legal_name,
        "addressLine1": s.address_line1,
        "addressLine2": s.address_line2,
        "postalCode": s.postal_code,
        "city": s.city,
        "country": s.country,
        "vatId": s.vat_id,
        "registrationNo": s.registration_no,
        "email": s.email,
        "phone": s.phone,
        "website": s.website,
        "iban": s.iban,
        "bic": s.bic,
        "bankName": s.bank_name,
        "accountHolder": s.account_holder,
        "footerNote": s.footer_note,
        // The currency the tenant keeps books in (B1.21). Never blank, even
        // unstated: a VAT summary has to be able to say what it converted into.
        "baseCurrency": s.base_currency,
        "updatedBy": s.updated_by,
        "updatedAt": s.updated_at.map(iso),
    })
}

/// The stored identity as writable input — the base a `PATCH` merges onto.
fn editable(s: &BillingSettings) -> NewBillingSettings {
    NewBillingSettings {
        legal_name: s.legal_name.clone(),
        address_line1: s.address_line1.clone(),
        address_line2: s.address_line2.clone(),
        postal_code: s.postal_code.clone(),
        city: s.city.clone(),
        country: s.country.clone(),
        vat_id: s.vat_id.clone(),
        registration_no: s.registration_no.clone(),
        email: s.email.clone(),
        phone: s.phone.clone(),
        website: s.website.clone(),
        iban: s.iban.clone(),
        bic: s.bic.clone(),
        bank_name: s.bank_name.clone(),
        account_holder: s.account_holder.clone(),
        footer_note: s.footer_note.clone(),
        base_currency: s.base_currency.clone(),
    }
}

/// The writable parts of the identity, every one optional.
///
/// Unknown fields are ignored so the contract can grow additively. `stated`,
/// `updatedBy` and `updatedAt` are not writable — they are facts about the
/// row, not fields of it.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct SettingsBody {
    #[serde(default)]
    legal_name: Option<String>,
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
    #[serde(default)]
    email: Option<String>,
    #[serde(default)]
    phone: Option<String>,
    #[serde(default)]
    website: Option<String>,
    #[serde(default, deserialize_with = "absent_or_null")]
    iban: Option<Option<String>>,
    #[serde(default, deserialize_with = "absent_or_null")]
    bic: Option<Option<String>>,
    #[serde(default)]
    bank_name: Option<String>,
    #[serde(default)]
    account_holder: Option<String>,
    #[serde(default)]
    footer_note: Option<String>,
    #[serde(default)]
    base_currency: Option<String>,
}

impl SettingsBody {
    /// Merges the stated fields onto `base`, leaving the rest as they were.
    fn apply(self, base: NewBillingSettings) -> NewBillingSettings {
        NewBillingSettings {
            legal_name: self.legal_name.unwrap_or(base.legal_name),
            address_line1: self.address_line1.unwrap_or(base.address_line1),
            address_line2: self.address_line2.unwrap_or(base.address_line2),
            postal_code: self.postal_code.unwrap_or(base.postal_code),
            city: self.city.unwrap_or(base.city),
            country: self.country.unwrap_or(base.country),
            vat_id: self.vat_id.map_or(base.vat_id, blank_to_none),
            registration_no: self.registration_no.unwrap_or(base.registration_no),
            email: self.email.unwrap_or(base.email),
            phone: self.phone.unwrap_or(base.phone),
            website: self.website.unwrap_or(base.website),
            iban: self.iban.map_or(base.iban, blank_to_none),
            bic: self.bic.map_or(base.bic, blank_to_none),
            bank_name: self.bank_name.unwrap_or(base.bank_name),
            account_holder: self.account_holder.unwrap_or(base.account_holder),
            footer_note: self.footer_note.unwrap_or(base.footer_note),
            base_currency: self.base_currency.unwrap_or(base.base_currency),
        }
    }
}

/// `GET /billing/settings` → `{"settings":{…}}` — who this tenant invoices as.
///
/// Never `404`: an unstated identity is the blanks with `stated: false`.
pub async fn get_settings(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let settings = account
        .acc
        .billing_settings()
        .await
        .map_err(map_store_err)?;
    Ok(Json(json!({ "settings": settings_json(&settings) })))
}

/// `PATCH /billing/settings` `{…}` → `{"settings":{…}}` — save it.
///
/// The stated fields merge onto the stored ones; a `null` clears a nullable
/// field (that is how a VAT id or an IBAN comes off). The legal name is
/// required by the store, so the very first save must carry one.
pub async fn update_settings(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: axum::body::Bytes,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let req: SettingsBody = parse_body(&body)?;
    let stored = account
        .acc
        .billing_settings()
        .await
        .map_err(map_store_err)?;
    let input = req.apply(editable(&stored));
    let settings = account
        .acc
        .save_billing_settings(&input)
        .await
        .map_err(map_store_err)?;
    Ok(Json(json!({ "settings": settings_json(&settings) })))
}
