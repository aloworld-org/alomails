//! The owner's side of catalog order forms at the HTTP edge (ADR 0036; the
//! no-checkout order form of ADR 0041): reading the order inbox, moving an
//! order through the workflow, deleting one, and exporting the lot.
//!
//! A separate module from [`crate::sites`] for a separate reason to change:
//! these routes answer about *what a visitor asked to buy*, and they are the
//! only `/sites/{id}` routes whose rows carry a member of the public's name,
//! address and phone number. Everything here goes through the account door, so
//! an id from another tenant — or from another site of the same tenant — is
//! indistinguishable from one that never existed.
//!
//! Errors follow the `/sites/{id}` contract: `401` unauthenticated, `404` for
//! anything that does not resolve in the caller's tenant, `422` for a rule the
//! store names, `400` for a body that is not the shape.
//!
//! There is no create route by design. The only writer of an order is the
//! anonymous public door on `alo-sites`, which prices it from the publish; a
//! tenant-side "add order" would be a second, unpriced way in.

use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::{Json, body::Bytes};
use serde::Deserialize;
use serde_json::{Value, json};

use alo_store::{
    SiteId, SiteOrder, SiteOrderId, SiteOrderLine, SiteOrderStatus, currency_exponent,
};

use crate::billing::iso;
use crate::error::Problem;
use crate::sites::map_store_err;
use crate::state::{Account, AppState, authenticate};

/// One order with the lines that belong to it, as the inbox reads it.
///
/// `currencyExponent` travels beside the currency, exactly as it does on a
/// catalog: every figure here is integer minor units, and the screen that
/// shows them must not own a second copy of the ISO 4217 exception table —
/// a yen order has no decimals and a euro order has two, and only the server
/// knows which.
fn order_json(order: &SiteOrder, lines: &[SiteOrderLine]) -> Value {
    json!({
        "id": order.id.as_str(),
        "catalogId": order.catalog_id,
        "catalogName": order.catalog_name,
        "currency": order.currency,
        "currencyExponent": currency_exponent(&order.currency),
        "customerName": order.customer_name,
        "customerEmail": order.customer_email,
        "customerPhone": order.customer_phone,
        "note": order.note,
        "totalCents": order.total_cents,
        "status": order.status.as_str(),
        "receivedAt": iso(order.received_at),
        "lines": lines.iter().map(|line| json!({
            "itemSlug": line.item_slug,
            "itemName": line.item_name,
            "quantity": line.quantity,
            "unitPriceCents": line.unit_price_cents,
            "lineTotalCents": line.line_total_cents,
        })).collect::<Vec<_>>(),
    })
}

/// The site's orders with their lines, newest first — two reads for the whole
/// inbox rather than one per order.
async fn site_orders_with_lines(
    account: &Account,
    site: &SiteId,
) -> Result<(Vec<SiteOrder>, Vec<(SiteOrderId, SiteOrderLine)>), Problem> {
    let orders = account.acc.site_orders(site).await.map_err(map_store_err)?;
    let lines = account
        .acc
        .site_all_order_lines(site)
        .await
        .map_err(map_store_err)?;
    Ok((orders, lines))
}

fn lines_of<'a>(
    lines: &'a [(SiteOrderId, SiteOrderLine)],
    order: &SiteOrderId,
) -> Vec<&'a SiteOrderLine> {
    lines
        .iter()
        .filter(|(id, _)| id.as_str() == order.as_str())
        .map(|(_, line)| line)
        .collect()
}

/// `GET /sites/:id/orders` -> every order for the site, newest first, each
/// with what was asked for. A site that is not the caller's tenant's is a 404.
pub async fn list_orders(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let site = SiteId::new(id);
    require_site(&account, &site).await?;
    let (orders, lines) = site_orders_with_lines(&account, &site).await?;
    Ok(Json(json!({
        "orders": orders.iter().map(|order| {
            let own: Vec<SiteOrderLine> = lines_of(&lines, &order.id)
                .into_iter()
                .cloned()
                .collect();
            order_json(order, &own)
        }).collect::<Vec<_>>()
    })))
}

#[derive(Deserialize)]
struct StatusBody {
    status: String,
}

/// `PUT /sites/:id/orders/:order` `{status}` -> the order as it now stands.
/// Every transition is allowed in both directions: an order cancelled by
/// mistake is confirmed again rather than re-typed.
pub async fn set_order_status(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((id, order)): Path<(String, String)>,
    body: Bytes,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let site = SiteId::new(id);
    let order = SiteOrderId::new(order);
    let body: StatusBody = serde_json::from_slice(&body)
        .map_err(|_| Problem::with(StatusCode::BAD_REQUEST, "expected {\"status\": \"…\"}"))?;
    let status = SiteOrderStatus::parse(&body.status).map_err(map_store_err)?;
    account
        .acc
        .set_site_order_status(&site, &order, status)
        .await
        .map_err(map_store_err)?;
    let stored = account
        .acc
        .site_order(&site, &order)
        .await
        .map_err(map_store_err)?
        .ok_or_else(|| Problem::with(StatusCode::NOT_FOUND, "no such order"))?;
    let lines = account
        .acc
        .site_order_lines(&site, &order)
        .await
        .map_err(map_store_err)?;
    Ok(Json(order_json(&stored, &lines)))
}

/// `DELETE /sites/:id/orders/:order` -> `204`. Spam, a duplicate, or a
/// customer asking for their data to be removed: an order carries their name,
/// address and phone number, so deleting it must actually delete it — the
/// lines go with it.
pub async fn delete_order(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((id, order)): Path<(String, String)>,
) -> Result<StatusCode, Problem> {
    let account = authenticate(&state, &headers).await?;
    account
        .acc
        .delete_site_order(&SiteId::new(id), &SiteOrderId::new(order))
        .await
        .map_err(map_store_err)?;
    Ok(StatusCode::NO_CONTENT)
}

/// `GET /sites/:id/orders.csv` -> the same tenant-scoped inbox as a
/// spreadsheet-ready download, **one row per ordered line** so the numbers can
/// be summed. The order's own columns repeat on each of its lines, which is
/// what a baker filtering by item on Saturday morning needs.
pub async fn export_orders(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Response, Problem> {
    let account = authenticate(&state, &headers).await?;
    let site = SiteId::new(id);
    let stored_site = require_site(&account, &site).await?;
    let (orders, lines) = site_orders_with_lines(&account, &site).await?;
    let mut body = crate::csv::row(&[
        "receivedAt",
        "orderId",
        "status",
        "customerName",
        "customerEmail",
        "customerPhone",
        "note",
        "item",
        "quantity",
        "unitPriceCents",
        "lineTotalCents",
        "orderTotalCents",
        "currency",
    ]);
    for order in &orders {
        let received_at = iso(order.received_at);
        let customer_name = csv_text(&order.customer_name);
        let customer_email = csv_text(&order.customer_email);
        let customer_phone = csv_text(order.customer_phone.as_deref().unwrap_or_default());
        let note = csv_text(order.note.as_deref().unwrap_or_default());
        let order_total = order.total_cents.to_string();
        for line in lines_of(&lines, &order.id) {
            let item = csv_text(&line.item_name);
            let quantity = line.quantity.to_string();
            let unit_price = line
                .unit_price_cents
                .map(|cents| cents.to_string())
                .unwrap_or_default();
            let line_total = line
                .line_total_cents
                .map(|cents| cents.to_string())
                .unwrap_or_default();
            body.push_str(&crate::csv::row(&[
                &received_at,
                order.id.as_str(),
                order.status.as_str(),
                &customer_name,
                &customer_email,
                &customer_phone,
                &note,
                &item,
                &quantity,
                &unit_price,
                &line_total,
                &order_total,
                &order.currency,
            ]));
        }
    }
    let file_name = format!("orders-{}.csv", stored_site.subdomain);
    Ok((
        [
            (header::CONTENT_TYPE, "text/csv; charset=utf-8".to_owned()),
            (
                header::CONTENT_DISPOSITION,
                format!("attachment; filename=\"{file_name}\""),
            ),
            (header::X_CONTENT_TYPE_OPTIONS, "nosniff".to_owned()),
            (header::CACHE_CONTROL, "no-store".to_owned()),
        ],
        body,
    )
        .into_response())
}

/// Resolves the site through the account door, or the tenant-hidden 404.
async fn require_site(account: &Account, site: &SiteId) -> Result<alo_store::sites::Site, Problem> {
    account
        .acc
        .site(site)
        .await
        .map_err(map_store_err)?
        .ok_or_else(|| Problem::with(StatusCode::NOT_FOUND, "no such site"))
}

/// Neutralises user-authored text before it reaches a spreadsheet cell — a
/// customer's note is prose typed by a stranger, and Excel evaluates a cell
/// that starts with `=`, `+`, `-` or `@` as a formula. Mirrors the same rule
/// on the submissions export.
fn csv_text(value: &str) -> String {
    if value
        .trim_start()
        .chars()
        .next()
        .is_some_and(|first| matches!(first, '=' | '+' | '-' | '@'))
    {
        format!("'{value}")
    } else {
        value.to_owned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_note_that_looks_like_a_formula_is_neutralised() {
        assert_eq!(csv_text("=cmd|'/c calc'"), "'=cmd|'/c calc'");
        assert_eq!(csv_text("  +1 555 0100"), "'  +1 555 0100");
        assert_eq!(csv_text("leave at the door"), "leave at the door");
    }
}
