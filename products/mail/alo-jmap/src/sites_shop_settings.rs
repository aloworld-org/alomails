//! The site's own shop settings on the wire (ADR 0041, S3.05b3): today that
//! is one fact, the flat per-order delivery price.
//!
//! A separate module from [`crate::sites_tickets`] for a separate reason to
//! change: these routes manage what the *site itself* charges around the
//! goods, not what is on sale. The rate is the site's own price
//! (`site_shop_settings`), stored in integer cents like every price in alo —
//! it copies nothing from Billing, and Billing never reads it back. It was
//! store-only until the shop-setup approval screen needed a door to apply a
//! proposed rate through; wiring the existing store verbs is the whole job
//! here, the S3.04f3 precedent.
//!
//! Errors follow the `/sites/{id}` contract: `401` unauthenticated, `404` for
//! a site that does not resolve in the caller's tenant, `422` for a rate the
//! store refuses (negative, or over its ceiling) with the store's sentence
//! verbatim, `400` for a body that is not the shape. Reading the rate is a
//! collaborator's read — the public checkout states it to strangers — but
//! setting it is owner-only (`403`, S3.06a): it is a price the business
//! charges.

use axum::extract::{Path, State};
use axum::http::HeaderMap;
use axum::{Json, body::Bytes};
use serde::Deserialize;
use serde_json::{Value, json};

use alo_store::SiteId;

use crate::billing::parse_body;
use crate::error::Problem;
use crate::sites::{map_store_err, require_commerce_site, require_site};
use crate::state::{AppState, authenticate};

/// The one answer both verbs give: the settings as stored.
fn settings_json(shipping_cents: i64) -> Value {
    json!({ "shippingCents": shipping_cents })
}

/// `GET /sites/:id/shop-settings` → `{"shippingCents": …}`. A site that has
/// never set a rate answers `0` — ships for nothing, like a counter you
/// collect at — because that is what the public checkout would charge it.
pub async fn get_settings(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let site = SiteId::new(id);
    require_site(&account, &site).await?;
    let cents = account
        .acc
        .site_shop_shipping_cents(&site)
        .await
        .map_err(map_store_err)?;
    Ok(Json(settings_json(cents)))
}

/// The writable settings. `shippingCents` is required: there is one field,
/// and a body that does not state it has nothing to say.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct SettingsBody {
    shipping_cents: i64,
}

/// `PUT /sites/:id/shop-settings` `{"shippingCents": …}` → the stored
/// settings. Bounds are the store's; its refusal sentence travels verbatim.
pub async fn set_settings(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
    body: Bytes,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let site = SiteId::new(id);
    require_commerce_site(&account, &site).await?;
    let req: SettingsBody = parse_body(&body)?;
    account
        .acc
        .set_site_shop_shipping_cents(&site, req.shipping_cents)
        .await
        .map_err(map_store_err)?;
    let cents = account
        .acc
        .site_shop_shipping_cents(&site)
        .await
        .map_err(map_store_err)?;
    Ok(Json(settings_json(cents)))
}
