//! The owner's side of the stock shop (ADR 0041, S3.05c): which of the
//! tenant's stocked products a site lists for sale.
//!
//! A separate module from [`crate::sites_tickets`] for a separate reason to
//! change: a ticketed event is dated and carries its own capacity, a shop
//! listing is a bare reference whose capacity is the warehouse's ledger. The
//! discipline is the same one wave one set:
//!
//! * **The product is named, never described.** A listing stores Billing's
//!   product id and nothing else; the name, the price and the shelf count in
//!   every answer here are the owning seams' answers *now*, and a listing
//!   whose product has left the price list says so (`productName: null`)
//!   rather than showing a price that is no longer anyone's.
//! * **Adding is ruled on by the store.** The product must be on the active
//!   price list and stocked — the store's refusal sentences travel verbatim.
//! * **Delisting never touches a sale.** Orders keep their own product
//!   reference; removing a listing only takes it out of the shop window.
//!
//! Errors follow the `/sites/{id}` contract: `401` unauthenticated, `404` for
//! anything that does not resolve in the caller's tenant, `422` for a rule
//! the store names (an already-listed product included — conflicts speak as
//! 422 on this surface), `400` for a body that is not the shape.

use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode};
use axum::{Json, body::Bytes};
use serde::Deserialize;
use serde_json::{Value, json};
use time::OffsetDateTime;

use alo_store::{
    BillingProductId, SiteId, SiteShopCandidate, SiteShopItemId, SiteShopShelfRow,
    currency_exponent,
};

use crate::billing::iso;
use crate::error::Problem;
use crate::sites::{map_store_err, require_site};
use crate::state::{AppState, authenticate};

/// One shelf listing as the Shop screen shows it: the reference, and the
/// owning seams' answers at this read. The three `null`s of a product that
/// left the price list arrive together — the screen shows the honest state.
fn shelf_json(row: &SiteShopShelfRow) -> Value {
    json!({
        "id": row.id.as_str(),
        "productId": row.product.as_str(),
        "productName": row.item.as_ref().map(|item| item.name.clone()),
        "unit": row.item.as_ref().map(|item| item.unit.clone()),
        "unitPriceCents": row.item.as_ref().map(|item| item.unit_price_cents),
        "vatRateBp": row.item.as_ref().map(|item| item.vat_rate_bp),
        "availableUnits": row.available_units,
        "createdAt": iso(row.created_at),
    })
}

/// One product the add-product picker offers: on the price list, stocked,
/// with the shelf count a buyer would see. Zero units is still offered —
/// sold out is a state, not a refusal.
fn candidate_json(candidate: &SiteShopCandidate) -> Value {
    json!({
        "id": candidate.item.id.as_str(),
        "name": candidate.item.name,
        "unit": candidate.item.unit,
        "unitPriceCents": candidate.item.unit_price_cents,
        "vatRateBp": candidate.item.vat_rate_bp,
        "availableUnits": candidate.available_units,
    })
}

/// `GET /sites/:id/shop-products` -> the stocked items of the tenant's price
/// list, through the same seams the shop prices and reserves with. An empty
/// list is an honest answer the screen explains: nothing is stocked to sell.
pub async fn list_products(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let site = SiteId::new(id);
    require_site(&account, &site).await?;
    let (currency, candidates) = account
        .acc
        .site_shop_candidates(OffsetDateTime::now_utc())
        .await
        .map_err(map_store_err)?;
    Ok(Json(json!({
        "currency": currency,
        "currencyExponent": currency_exponent(&currency),
        "products": candidates.iter().map(candidate_json).collect::<Vec<_>>(),
    })))
}

/// `GET /sites/:id/shop-items` -> the site's shelf in listing order, each
/// row resolved by the owning seams at this read.
pub async fn list_items(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let site = SiteId::new(id);
    require_site(&account, &site).await?;
    let (currency, rows) = account
        .acc
        .site_shop_shelf(&site, OffsetDateTime::now_utc())
        .await
        .map_err(map_store_err)?;
    Ok(Json(json!({
        "currency": currency,
        "currencyExponent": currency_exponent(&currency),
        "items": rows.iter().map(shelf_json).collect::<Vec<_>>(),
    })))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct AddItemBody {
    product_id: String,
}

/// `POST /sites/:id/shop-items` -> the stored listing, resolved like the
/// list shows it. The store rules on the product: not on the price list,
/// not stocked, or already listed is a `422` in its own words.
pub async fn add_item(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
    body: Bytes,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let req: AddItemBody = serde_json::from_slice(&body).map_err(|_| Problem::not_json())?;
    let site = SiteId::new(id);
    require_site(&account, &site).await?;
    let product = BillingProductId::new(req.product_id.trim().to_owned());
    let now = OffsetDateTime::now_utc();
    let created = account
        .acc
        .add_site_shop_item(&site, &product, now)
        .await
        .map_err(map_store_err)?;
    let (currency, rows) = account
        .acc
        .site_shop_shelf(&site, now)
        .await
        .map_err(map_store_err)?;
    let stored = rows
        .iter()
        .find(|row| row.id == created)
        .ok_or_else(|| Problem::with(StatusCode::NOT_FOUND, "no such listing"))?;
    Ok(Json(json!({
        "currency": currency,
        "currencyExponent": currency_exponent(&currency),
        "item": shelf_json(stored),
    })))
}

/// `DELETE /sites/:id/shop-items/:item` -> `204`. Orders already placed keep
/// their own product reference, so delisting never touches a sale.
pub async fn remove_item(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((id, item)): Path<(String, String)>,
) -> Result<StatusCode, Problem> {
    let account = authenticate(&state, &headers).await?;
    let site = SiteId::new(id);
    require_site(&account, &site).await?;
    account
        .acc
        .remove_site_shop_item(&site, &SiteShopItemId::new(item))
        .await
        .map_err(map_store_err)?;
    Ok(StatusCode::NO_CONTENT)
}
