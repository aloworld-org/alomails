//! The owner's side of the ticket shop (ADR 0041, S3.04f3): the dated events
//! a site sells seats to, and the price-list items an event may sell.
//!
//! A separate module from [`crate::sites`] for a separate reason to change:
//! these routes manage what is *on sale* — the public buying flow lives on
//! `alo-sites`, and the seat arithmetic that makes overselling impossible
//! lives in the store's hold machinery. Three facts shape the wire contract:
//!
//! * **The product is named, never described.** An event stores a
//!   `productId` into Billing's price list; the name and the price in every
//!   answer here are the catalog seam's answer *now*, and an event whose item
//!   has been archived says so (`productName: null`) rather than showing a
//!   price that is no longer anyone's.
//! * **Capacity is the only edit.** When an event sells, its date and its
//!   product are what people bought; the store offers no verb to change them
//!   and neither does this surface. Growing capacity is always allowed,
//!   shrinking stops at the seats already spoken for.
//! * **Sold seats are a record.** Deleting is refused once a seat is sold —
//!   the refusal sentence is the store's own and travels verbatim.
//!
//! Errors follow the `/sites/{id}` contract: `401` unauthenticated, `404` for
//! anything that does not resolve in the caller's tenant, `422` for a rule
//! the store names, `400` for a body that is not the shape. Everything but
//! the event list is owner-only (`403`, S3.06a): the picker reads the whole
//! price list and the verbs decide what is on sale — a restricted
//! collaborator (S2.03a) edits pages, not the business behind them.

use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode};
use axum::{Json, body::Bytes};
use serde::Deserialize;
use serde_json::{Value, json};
use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;

use alo_store::{
    BillingProductId, CatalogSaleItem, SiteId, SiteTicketEventId, TicketAvailability,
    currency_exponent,
};

use crate::billing::iso;
use crate::error::Problem;
use crate::sites::{map_store_err, require_commerce_site, require_site};
use crate::state::{Account, AppState, authenticate};

/// One price-list item as the event dialog offers it: the seam's answer now,
/// never a stored copy.
fn product_json(item: &CatalogSaleItem) -> Value {
    json!({
        "id": item.id.as_str(),
        "name": item.name,
        "unit": item.unit,
        "unitPriceCents": item.unit_price_cents,
        "vatRateBp": item.vat_rate_bp,
    })
}

/// One event with its product resolved and its seats counted. `productName`
/// (and the price with it) is `null` when the item has left the price list —
/// the owner needs that as a fact the screen can show, not as a stale price.
fn event_json(
    event: &alo_store::SiteTicketEvent,
    item: Option<&CatalogSaleItem>,
    sold: i64,
    held: i64,
) -> Value {
    json!({
        "id": event.id.as_str(),
        "productId": event.product.as_str(),
        "productName": item.map(|item| item.name.clone()),
        "unitPriceCents": item.map(|item| item.unit_price_cents),
        "vatRateBp": item.map(|item| item.vat_rate_bp),
        "startsAt": iso(event.starts_at),
        "capacity": event.capacity,
        "sold": sold,
        "held": held,
        "remaining": i64::from(event.capacity) - sold - held,
        "createdAt": iso(event.created_at),
        "updatedAt": iso(event.updated_at),
    })
}

/// `GET /sites/:id/ticket-products` -> the tenant's own price list through
/// the same seam the shop prices with: what an event may sell, and the list
/// currency. An empty list is an honest answer the dialog explains. Owner
/// only (S3.06a): this is the whole price list, not the site's slice of it.
pub async fn list_products(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let site = SiteId::new(id);
    require_commerce_site(&account, &site).await?;
    let (currency, items) = account
        .acc
        .site_ticket_sale_items()
        .await
        .map_err(map_store_err)?;
    Ok(Json(json!({
        "currency": currency,
        "currencyExponent": currency_exponent(&currency),
        "products": items.iter().map(product_json).collect::<Vec<_>>(),
    })))
}

/// `GET /sites/:id/tickets` -> every event of the site in start order, each
/// priced by the seam at this read and carrying its live seat arithmetic.
pub async fn list_events(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let site = SiteId::new(id);
    require_site(&account, &site).await?;
    let events = account
        .acc
        .site_ticket_events(&site)
        .await
        .map_err(map_store_err)?;
    // One read of the price list and one of the seat tally for the whole
    // screen, matched in memory: a full calendar must not cost 200 queries.
    let (currency, items) = account
        .acc
        .site_ticket_sale_items()
        .await
        .map_err(map_store_err)?;
    let counts = account
        .acc
        .site_ticket_seat_counts(&site, OffsetDateTime::now_utc())
        .await
        .map_err(map_store_err)?;
    let seats = |event: &SiteTicketEventId| {
        counts
            .iter()
            .find(|count| &count.event == event)
            .map_or((0, 0), |count| (count.sold, count.held))
    };
    Ok(Json(json!({
        "currency": currency,
        "currencyExponent": currency_exponent(&currency),
        "events": events
            .iter()
            .map(|event| {
                let item = items.iter().find(|item| item.id == event.product);
                let (sold, held) = seats(&event.id);
                event_json(event, item, sold, held)
            })
            .collect::<Vec<_>>(),
    })))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct CreateEventBody {
    product_id: String,
    /// RFC 3339 — the owner's picker sends an instant, the store keeps UTC.
    starts_at: String,
    capacity: i32,
}

/// `POST /sites/:id/tickets` -> the stored event, seats all free.
pub async fn create_event(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
    body: Bytes,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let req: CreateEventBody = serde_json::from_slice(&body).map_err(|_| Problem::not_json())?;
    let starts_at = OffsetDateTime::parse(req.starts_at.trim(), &Rfc3339).map_err(|_| {
        Problem::with(
            StatusCode::UNPROCESSABLE_ENTITY,
            "startsAt must be an RFC 3339 date-time",
        )
    })?;
    let site = SiteId::new(id);
    require_commerce_site(&account, &site).await?;
    let product = BillingProductId::new(req.product_id.trim().to_owned());
    let created = account
        .acc
        .create_site_ticket_event(&site, &product, starts_at, req.capacity)
        .await
        .map_err(map_store_err)?;
    answer(&account, &site, &created).await
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct CapacityBody {
    capacity: i32,
}

/// `PUT /sites/:id/tickets/:event` -> the event as it now stands. Capacity is
/// the only fact this verb changes; shrinking below the seats already sold or
/// on hold is the store's refusal, spoken verbatim.
pub async fn set_capacity(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((id, event)): Path<(String, String)>,
    body: Bytes,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let req: CapacityBody = serde_json::from_slice(&body).map_err(|_| Problem::not_json())?;
    let site = SiteId::new(id);
    require_commerce_site(&account, &site).await?;
    let event = SiteTicketEventId::new(event);
    account
        .acc
        .set_site_ticket_capacity(&site, &event, req.capacity, OffsetDateTime::now_utc())
        .await
        .map_err(map_store_err)?;
    answer(&account, &site, &event).await
}

/// `DELETE /sites/:id/tickets/:event` -> `204` while nobody has bought a
/// seat; afterwards the event is a record of a sale and the store refuses.
pub async fn delete_event(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((id, event)): Path<(String, String)>,
) -> Result<StatusCode, Problem> {
    let account = authenticate(&state, &headers).await?;
    let site = SiteId::new(id);
    require_commerce_site(&account, &site).await?;
    account
        .acc
        .delete_site_ticket_event(&site, &SiteTicketEventId::new(event))
        .await
        .map_err(map_store_err)?;
    Ok(StatusCode::NO_CONTENT)
}

/// Reads one event back and answers with it — the single shape create and
/// capacity return, so the screen never reconciles two spellings.
async fn answer(
    account: &Account,
    site: &SiteId,
    event: &SiteTicketEventId,
) -> Result<Json<Value>, Problem> {
    let stored = account
        .acc
        .site_ticket_event(site, event)
        .await
        .map_err(map_store_err)?
        .ok_or_else(|| Problem::with(StatusCode::NOT_FOUND, "no such event"))?;
    let TicketAvailability { sold, held, .. } = account
        .acc
        .ticket_availability(site, event, OffsetDateTime::now_utc())
        .await
        .map_err(map_store_err)?;
    let (currency, items) = account
        .acc
        .site_ticket_sale_items()
        .await
        .map_err(map_store_err)?;
    let item = items.iter().find(|item| item.id == stored.product);
    Ok(Json(json!({
        "currency": currency,
        "currencyExponent": currency_exponent(&currency),
        "event": event_json(&stored, item, sold, held),
    })))
}
