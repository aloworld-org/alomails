//! Billing products HTTP surface (alo Billing, ADR 0035, wave B1) — CRUD over
//! the tenant's price list on top of [`alo_store::billing_products`].
//!
//! The same conventions as [`crate::billing_customers`]: authenticated and
//! tenant-scoped through the account door, no validation duplicated from the
//! store, every write answered with the stored record, and `PATCH` as a merge
//! onto it.
//!
//! One rule is specific to this module. **Money is integer cents and rates are
//! basis points**, so `unitPriceCents` and `vatRateBp` are JSON integers: a
//! client that sends `19.99` gets a `400`, not a silently rounded price. That
//! refusal is serde's, deliberately left in place — the alternative is a price
//! list that disagrees with itself in the third decimal.
//!
//! The same routes carry the catalog fields alo Inventory adds (B5.02,
//! `docs/design/inventory.md`): `sku`, `barcode`, `stocked`,
//! `purchasePriceCents` and `photoNodeId`, all optional and all additive — a
//! client written against the billing contract keeps working, and a services
//! tenant never states any of them. Two refusals are worth knowing about:
//! a barcode whose check digit does not match is a `422` naming the field and
//! never the code, and an SKU or barcode another product of the **same
//! tenant** already carries is a `409`. Uniqueness is tenant-scoped, so two
//! businesses selling the same book never collide.
//!
//! A product is a **source** for a document line, not a dependency of one:
//! picking it copies name, unit, price and VAT rate onto the line at that
//! moment (B1.06). Editing a price here therefore never rewrites a document
//! already raised, which is also why a product is archived rather than deleted.

use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use serde::Deserialize;
use serde_json::{Value, json};

use alo_store::billing_products::{NewProduct, Product};
use alo_store::{AccountStore, BillingProductId, DriveNodeId};

use crate::billing::{absent_or_null, blank_to_none, flag, iso, map_store_err, parse_body};
use crate::error::Problem;
use crate::state::{AppState, authenticate};

/// A product as JSON. No currency field: a price list is quoted in the
/// tenant's own currency, and the document carries the currency it was raised
/// in (`docs/design/billing.md`).
fn product_json(p: &Product) -> Value {
    json!({
        "id": p.id.as_str(),
        "name": p.name,
        "unit": p.unit,
        "unitPriceCents": p.unit_price_cents,
        "vatRateBp": p.vat_rate_bp,
        // The catalog half (B5.02). `purchasePriceCents` is what we pay and
        // `unitPriceCents` is what we charge; both are integers for the same
        // reason, and the client computes neither.
        "sku": p.sku,
        "barcode": p.barcode,
        "stocked": p.stocked,
        "purchasePriceCents": p.purchase_price_cents,
        "photoNodeId": p.photo_node_id.as_ref().map(DriveNodeId::as_str),
        "archived": p.is_archived(),
        "archivedAt": p.archived_at.map(iso),
        "createdBy": p.created_by,
        "createdAt": iso(p.created_at),
        "updatedAt": iso(p.updated_at),
    })
}

/// The stored record as writable input — the base a `PATCH` merges onto.
fn editable(p: &Product) -> NewProduct {
    NewProduct {
        name: p.name.clone(),
        unit: p.unit.clone(),
        unit_price_cents: p.unit_price_cents,
        vat_rate_bp: p.vat_rate_bp,
        sku: p.sku.clone(),
        barcode: p.barcode.clone(),
        stocked: p.stocked,
        purchase_price_cents: p.purchase_price_cents,
        photo_node_id: p.photo_node_id.clone(),
    }
}

/// The writable fields of a product, every one optional.
///
/// The same body serves `POST` (merged onto [`NewProduct::default`] — a free,
/// zero-rated, unitless item) and `PATCH` (merged onto the stored record).
/// Unknown fields are ignored so the contract can grow additively; see
/// [`crate::billing_customers`] for the full reasoning.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ProductBody {
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    unit: Option<String>,
    #[serde(default)]
    unit_price_cents: Option<i64>,
    #[serde(default)]
    vat_rate_bp: Option<i32>,
    #[serde(default)]
    sku: Option<String>,
    #[serde(default)]
    barcode: Option<String>,
    #[serde(default)]
    stocked: Option<bool>,
    #[serde(default)]
    purchase_price_cents: Option<i64>,
    /// Nullable, so a photo attached by mistake can be taken off again — the
    /// three-way absent/`null`/value the plain `Option` cannot express.
    #[serde(default, deserialize_with = "absent_or_null")]
    photo_node_id: Option<Option<String>>,
}

impl ProductBody {
    /// Merges the stated fields onto `base`, leaving the rest as they were.
    fn apply(self, base: NewProduct) -> NewProduct {
        NewProduct {
            name: self.name.unwrap_or(base.name),
            unit: self.unit.unwrap_or(base.unit),
            unit_price_cents: self.unit_price_cents.unwrap_or(base.unit_price_cents),
            vat_rate_bp: self.vat_rate_bp.unwrap_or(base.vat_rate_bp),
            sku: self.sku.unwrap_or(base.sku),
            barcode: self.barcode.unwrap_or(base.barcode),
            stocked: self.stocked.unwrap_or(base.stocked),
            purchase_price_cents: self
                .purchase_price_cents
                .unwrap_or(base.purchase_price_cents),
            photo_node_id: self.photo_node_id.map_or(base.photo_node_id, |v| {
                blank_to_none(v).map(DriveNodeId::new)
            }),
        }
    }
}

/// Loads one of the tenant's products, or fails with the `404` an id from
/// another tenant gets.
async fn load(acc: &AccountStore, id: &BillingProductId) -> Result<Product, Problem> {
    acc.billing_product(id)
        .await
        .map_err(map_store_err)?
        .ok_or_else(|| Problem::with(StatusCode::NOT_FOUND, "no such product"))
}

/// Query string of the list route.
#[derive(Deserialize)]
pub struct ListQuery {
    /// `includeArchived=1` also returns archived items, sorted after the
    /// active ones. Read through [`flag`], so an unparseable value is simply
    /// off rather than a rejected request.
    #[serde(default, rename = "includeArchived")]
    include_archived: Option<String>,
}

/// `GET /billing/products[?includeArchived=1]` → `{"products":[…]}` — the price
/// list in name order, active items first.
pub async fn list_products(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(q): Query<ListQuery>,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let products = account
        .acc
        .billing_products(flag(q.include_archived.as_deref()))
        .await
        .map_err(map_store_err)?;
    Ok(Json(json!({
        "products": products.iter().map(product_json).collect::<Vec<_>>(),
    })))
}

/// `POST /billing/products` `{name, unitPriceCents, vatRateBp, …}` →
/// `{"product":{…}}` — create. Only `name` is required; an unstated price is
/// zero and an unstated rate is exempt, both of which are real cases.
pub async fn create_product(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: axum::body::Bytes,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let req: ProductBody = parse_body(&body)?;
    let input = req.apply(NewProduct::default());
    let id = account
        .acc
        .create_billing_product(&input)
        .await
        .map_err(map_store_err)?;
    let product = load(&account.acc, &id).await?;
    Ok(Json(json!({ "product": product_json(&product) })))
}

/// `GET /billing/products/{id}` → `{"product":{…}}`. Archived products are
/// readable by id, so a line copied from one last year can still be explained.
pub async fn get_product(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let product = load(&account.acc, &BillingProductId::new(id)).await?;
    Ok(Json(json!({ "product": product_json(&product) })))
}

/// `PATCH /billing/products/{id}` `{…}` → `{"product":{…}}` — merge the stated
/// fields onto the stored record. A new price applies to documents raised from
/// now on; lines already written keep the price they snapshotted.
pub async fn update_product(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
    body: axum::body::Bytes,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let req: ProductBody = parse_body(&body)?;
    let id = BillingProductId::new(id);
    let stored = load(&account.acc, &id).await?;
    let input = req.apply(editable(&stored));
    account
        .acc
        .update_billing_product(&id, &input)
        .await
        .map_err(map_store_err)?;
    let product = load(&account.acc, &id).await?;
    Ok(Json(json!({ "product": product_json(&product) })))
}

#[derive(Deserialize)]
struct ArchiveBody {
    /// `false` restores. Required when a body is sent; an **empty** body
    /// archives, because the route's name is already the intent.
    archived: bool,
}

/// `POST /billing/products/{id}/archive` `{"archived":true}` →
/// `{"product":{…}}` — take an item off the price list, or put it back.
///
/// Never a delete, and separate from `PATCH` on purpose: a price change must
/// not be able to drop an item out of the pickers by accident. Idempotent —
/// re-archiving keeps the original time.
pub async fn archive_product(
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
    let id = BillingProductId::new(id);
    account
        .acc
        .set_billing_product_archived(&id, req.archived)
        .await
        .map_err(map_store_err)?;
    let product = load(&account.acc, &id).await?;
    Ok(Json(json!({ "product": product_json(&product) })))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn body(json: Value) -> ProductBody {
        serde_json::from_value(json).unwrap_or_else(|e| panic!("body rejected: {e}"))
    }

    fn stored() -> NewProduct {
        NewProduct {
            name: "Consulting".to_owned(),
            unit: "hour".to_owned(),
            unit_price_cents: 12_500,
            vat_rate_bp: 2100,
            ..Default::default()
        }
    }

    /// A stored stocked item, so the catalog merges have something to leave
    /// alone.
    fn stored_chair() -> NewProduct {
        NewProduct {
            name: "Blue chair".to_owned(),
            unit: "piece".to_owned(),
            unit_price_cents: 4_900,
            vat_rate_bp: 2100,
            sku: "CH-BLUE-01".to_owned(),
            barcode: "4006381333931".to_owned(),
            stocked: true,
            purchase_price_cents: 2_150,
            photo_node_id: Some(alo_store::DriveNodeId::new("node-1".to_owned())),
        }
    }

    #[test]
    fn an_empty_patch_changes_nothing() {
        let merged = body(json!({})).apply(stored());
        assert_eq!(merged.name, "Consulting");
        assert_eq!(merged.unit, "hour");
        assert_eq!(merged.unit_price_cents, 12_500);
        assert_eq!(merged.vat_rate_bp, 2100);
    }

    #[test]
    fn a_price_edit_leaves_the_rest_of_the_item_alone() {
        let merged = body(json!({ "unitPriceCents": 13_000 })).apply(stored());
        assert_eq!(merged.unit_price_cents, 13_000);
        assert_eq!(merged.name, "Consulting");
        assert_eq!(merged.vat_rate_bp, 2100);
    }

    #[test]
    fn zero_is_a_stated_value_not_an_absent_one() {
        // Free items and exempt rates are real; neither may fall back to the
        // stored value just because it is falsy.
        let merged = body(json!({ "unitPriceCents": 0, "vatRateBp": 0 })).apply(stored());
        assert_eq!(merged.unit_price_cents, 0);
        assert_eq!(merged.vat_rate_bp, 0);
    }

    #[test]
    fn create_starts_from_a_free_zero_rated_item() {
        let merged = body(json!({ "name": "Consulting" })).apply(NewProduct::default());
        assert_eq!(merged.name, "Consulting");
        assert_eq!(merged.unit, "");
        assert_eq!(merged.unit_price_cents, 0);
        assert_eq!(merged.vat_rate_bp, 0);
    }

    #[test]
    fn an_empty_patch_leaves_the_catalog_fields_alone() {
        let merged = body(json!({})).apply(stored_chair());
        assert_eq!(merged.sku, "CH-BLUE-01");
        assert_eq!(merged.barcode, "4006381333931");
        assert!(merged.stocked);
        assert_eq!(merged.purchase_price_cents, 2_150);
        assert_eq!(
            merged
                .photo_node_id
                .as_ref()
                .map(alo_store::DriveNodeId::as_str),
            Some("node-1")
        );
    }

    #[test]
    fn a_photo_can_be_taken_off_again() {
        // `null` clears; a blank string is what a cleared form field sends and
        // means the same thing; absent leaves it alone (above).
        for cleared in [json!({"photoNodeId": null}), json!({"photoNodeId": ""})] {
            let merged = body(cleared.clone()).apply(stored_chair());
            assert!(merged.photo_node_id.is_none(), "not cleared by {cleared}");
        }
        let set = body(json!({"photoNodeId": "node-2"})).apply(stored_chair());
        assert_eq!(
            set.photo_node_id
                .as_ref()
                .map(alo_store::DriveNodeId::as_str),
            Some("node-2")
        );
    }

    #[test]
    fn unstocking_and_clearing_a_code_are_stated_values() {
        // `false` and `""` are decisions here, not absences: an item taken out
        // of the stock ledger, and an SKU removed because it was wrong.
        let merged =
            body(json!({"stocked": false, "sku": "", "barcode": ""})).apply(stored_chair());
        assert!(!merged.stocked);
        assert_eq!(merged.sku, "");
        assert_eq!(merged.barcode, "");
    }

    #[test]
    fn create_starts_from_a_service_with_no_codes() {
        let merged = body(json!({ "name": "Consulting" })).apply(NewProduct::default());
        assert!(!merged.stocked, "nothing is stocked until somebody says so");
        assert_eq!(merged.purchase_price_cents, 0);
        assert!(merged.sku.is_empty());
        assert!(merged.barcode.is_empty());
        assert!(merged.photo_node_id.is_none());
    }

    #[test]
    fn a_price_with_a_decimal_point_is_refused_never_rounded() {
        assert!(serde_json::from_value::<ProductBody>(json!({"unitPriceCents": 19.99})).is_err());
        assert!(serde_json::from_value::<ProductBody>(json!({"unitPriceCents": "1999"})).is_err());
        assert!(serde_json::from_value::<ProductBody>(json!({"vatRateBp": 21.0})).is_err());
        assert!(
            serde_json::from_value::<ProductBody>(json!({"purchasePriceCents": 21.5})).is_err(),
            "what we pay is integer cents too"
        );
    }

    #[test]
    fn a_barcode_stays_a_string_on_the_wire() {
        // A JSON number would eat the leading zeros a GTIN carries, which is
        // the bug the text column exists to prevent.
        assert!(
            serde_json::from_value::<ProductBody>(json!({"barcode": 4006381333931u64}).clone())
                .is_err()
        );
        let merged = body(json!({"barcode": "012345678905"})).apply(NewProduct::default());
        assert_eq!(merged.barcode, "012345678905");
    }
}
