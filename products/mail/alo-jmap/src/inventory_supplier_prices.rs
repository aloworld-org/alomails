//! The offers a supplier quotes us, as HTTP (alo Inventory, ADR 0035, wave
//! B5.03) — the sub-resource under a supplier, on top of
//! [`alo_store::inv_supplier_prices`].
//!
//! Separate from [`crate::inventory_suppliers`] because it is a different
//! record with a different lifecycle: the master record is archived and never
//! deleted, while an offer is simply written over or removed. One file, one
//! reason to change.
//!
//! The write is a `PUT` on the pair `(supplier, product)` and is **idempotent
//! by construction** — the store upserts — so a form saves in one call and a
//! retried request cannot produce two quotes for the same product. The body is
//! a full statement of the offer rather than a merge: the resource *is* the
//! offer, and a partial `PUT` would leave a price and a currency disagreeing
//! about which quote they belong to.
//!
//! Both ends are the tenant's, checked in the store before anything is written:
//! another tenant's supplier and another tenant's product both answer `404`,
//! exactly as ids that never existed.

use axum::Json;
use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode};
use serde::Deserialize;
use serde_json::{Value, json};

use alo_store::inv_supplier_prices::{NewSupplierPrice, SupplierPrice};
use alo_store::{BillingProductId, InvSupplierId};

use crate::billing::{iso, map_store_err, parse_body};
use crate::error::Problem;
use crate::inventory_suppliers::load;
use crate::state::{AppState, authenticate};

/// One offer as JSON.
///
/// `leadTimeDays` is `null` when the offer inherits the supplier's default;
/// `effectiveLeadTimeDays` is the number a screen should actually show, so no
/// client has to re-implement the fallback (and none can get it wrong).
pub(crate) fn price_json(p: &SupplierPrice, supplier_default: i32) -> Value {
    json!({
        "supplierId": p.supplier_id.as_str(),
        "productId": p.product_id.as_str(),
        "productName": p.product_name,
        "supplierCode": p.supplier_code,
        "purchasePriceCents": p.purchase_price_cents,
        "currency": p.currency,
        "minOrderQtyMilli": p.min_order_qty_milli,
        "leadTimeDays": p.lead_time_days,
        "effectiveLeadTimeDays": p.effective_lead_time_days(supplier_default),
        "createdBy": p.created_by,
        "createdAt": iso(p.created_at),
        "updatedAt": iso(p.updated_at),
    })
}

/// The whole offer, as stated. Every field optional so a caller can send only
/// what they know; what is not sent takes the default (a free offer in euro,
/// no minimum, the supplier's own lead time) rather than a stored value —
/// this is a `PUT`, not a merge.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct PriceBody {
    #[serde(default)]
    supplier_code: Option<String>,
    #[serde(default)]
    purchase_price_cents: Option<i64>,
    #[serde(default)]
    currency: Option<String>,
    #[serde(default)]
    min_order_qty_milli: Option<i64>,
    /// Absent or `null` both mean "as the supplier says" — the two are the
    /// same fact here, which is why this needs no absent/null distinction.
    #[serde(default)]
    lead_time_days: Option<i32>,
}

impl PriceBody {
    /// The offer this body states, with the defaults for everything it does
    /// not.
    fn into_offer(self) -> NewSupplierPrice {
        let base = NewSupplierPrice::default();
        NewSupplierPrice {
            supplier_code: self.supplier_code.unwrap_or(base.supplier_code),
            purchase_price_cents: self
                .purchase_price_cents
                .unwrap_or(base.purchase_price_cents),
            currency: self.currency.unwrap_or(base.currency),
            min_order_qty_milli: self.min_order_qty_milli.unwrap_or(base.min_order_qty_milli),
            lead_time_days: self.lead_time_days,
        }
    }
}

/// `GET /inventory/suppliers/{id}/products` → `{"offers":[…]}` — what this
/// supplier sells us, in product-name order, each with the lead time that
/// actually applies.
pub async fn list_supplier_products(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let id = InvSupplierId::new(id);
    // Through the loader, so a supplier of another tenant is the same `404` as
    // one that never existed — and so the default lead time is the one this
    // supplier actually publishes.
    let supplier = load(&account.acc, &id).await?;
    let offers = account
        .acc
        .inv_supplier_prices(&id)
        .await
        .map_err(map_store_err)?;
    Ok(Json(json!({
        "offers": offers
            .iter()
            .map(|o| price_json(o, supplier.lead_time_days))
            .collect::<Vec<_>>(),
    })))
}

/// `PUT /inventory/suppliers/{id}/products/{productId}` `{…}` →
/// `{"offer":{…}}` — record what they quote, replacing any earlier quote for
/// the same product.
pub async fn set_supplier_product(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((id, product_id)): Path<(String, String)>,
    body: axum::body::Bytes,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    // An empty body is a legitimate "they sell it, price to follow": the pair
    // in the path is the whole statement, and every field has a default.
    let req: PriceBody = parse_body(if body.is_empty() { b"{}" } else { &body })?;
    let supplier_id = InvSupplierId::new(id);
    let product_id = BillingProductId::new(product_id);
    account
        .acc
        .set_inv_supplier_price(&supplier_id, &product_id, &req.into_offer())
        .await
        .map_err(map_store_err)?;
    let supplier = load(&account.acc, &supplier_id).await?;
    let offer = account
        .acc
        .inv_supplier_prices(&supplier_id)
        .await
        .map_err(map_store_err)?
        .into_iter()
        .find(|o| o.product_id == product_id)
        .ok_or_else(|| Problem::with(StatusCode::NOT_FOUND, "no such offer"))?;
    Ok(Json(json!({
        "offer": price_json(&offer, supplier.lead_time_days),
    })))
}

/// `DELETE /inventory/suppliers/{id}/products/{productId}` → `{"removed":true}`
/// — they no longer sell it, or never did.
///
/// A `404` when there is no such offer, which is also the answer for another
/// tenant's supplier. Safe in a way deleting a supplier is not: an order
/// already placed copied the price onto its line, so nothing that has happened
/// depends on this row.
pub async fn remove_supplier_product(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((id, product_id)): Path<(String, String)>,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    account
        .acc
        .remove_inv_supplier_price(&InvSupplierId::new(id), &BillingProductId::new(product_id))
        .await
        .map_err(map_store_err)?;
    Ok(Json(json!({ "removed": true })))
}

#[cfg(test)]
mod tests {
    use super::*;
    use time::OffsetDateTime;

    fn body(json: Value) -> PriceBody {
        serde_json::from_value(json).unwrap_or_else(|e| panic!("body rejected: {e}"))
    }

    fn offer(lead_time_days: Option<i32>) -> SupplierPrice {
        SupplierPrice {
            supplier_id: InvSupplierId::new("s"),
            product_id: BillingProductId::new("p"),
            product_name: "Blue chair".to_owned(),
            supplier_code: "HM-4471".to_owned(),
            purchase_price_cents: 315,
            currency: "EUR".to_owned(),
            min_order_qty_milli: 10_000,
            lead_time_days,
            created_by: "u".to_owned(),
            created_at: OffsetDateTime::UNIX_EPOCH,
            updated_at: OffsetDateTime::UNIX_EPOCH,
        }
    }

    #[test]
    fn an_empty_put_is_a_free_offer_with_the_suppliers_lead_time() {
        let stated = body(json!({})).into_offer();
        assert_eq!(stated.purchase_price_cents, 0);
        assert_eq!(stated.currency, "EUR");
        assert_eq!(stated.min_order_qty_milli, 0);
        assert!(stated.lead_time_days.is_none());
        assert!(stated.supplier_code.is_empty());
    }

    #[test]
    fn a_put_states_the_whole_offer_and_never_merges() {
        // Only the price is sent: everything else goes back to its default,
        // because the resource IS the offer.
        let stated = body(json!({ "purchasePriceCents": 299 })).into_offer();
        assert_eq!(stated.purchase_price_cents, 299);
        assert_eq!(stated.supplier_code, "");
        assert!(stated.lead_time_days.is_none());
    }

    #[test]
    fn money_and_quantity_are_integers_on_the_wire() {
        // €3.15 is 315 cents; a client that sends 3.15 gets a 400, not a
        // silently rounded price.
        assert!(serde_json::from_value::<PriceBody>(json!({"purchasePriceCents": 3.15})).is_err());
        assert!(serde_json::from_value::<PriceBody>(json!({"minOrderQtyMilli": 0.5})).is_err());
        assert!(serde_json::from_value::<PriceBody>(json!({"leadTimeDays": "9"})).is_err());
    }

    #[test]
    fn the_effective_lead_time_is_answered_by_the_server() {
        // The fallback lives in one place, so no client can get it wrong.
        let inherits = price_json(&offer(None), 14);
        assert_eq!(inherits["leadTimeDays"], Value::Null);
        assert_eq!(inherits["effectiveLeadTimeDays"], 14);
        let states = price_json(&offer(Some(9)), 14);
        assert_eq!(states["leadTimeDays"], 9);
        assert_eq!(states["effectiveLeadTimeDays"], 9);
        // Zero on the offer is same-day, not an absence.
        assert_eq!(price_json(&offer(Some(0)), 14)["effectiveLeadTimeDays"], 0);
    }

    #[test]
    fn the_offer_reports_money_as_the_integer_it_is() {
        let rendered = price_json(&offer(Some(9)), 14);
        assert_eq!(rendered["purchasePriceCents"], 315);
        assert_eq!(rendered["minOrderQtyMilli"], 10_000);
        assert_eq!(rendered["productName"], "Blue chair");
    }
}
