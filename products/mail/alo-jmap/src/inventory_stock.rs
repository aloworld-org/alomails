//! On-hand over HTTP (alo Inventory, ADR 0035, wave B5.04b) — what is where,
//! and what it is worth, over [`alo_store::inv_stock`].
//!
//! There is no writable surface here and there never will be: **on-hand is
//! derived**, the way a balance is derived from postings, and the only door
//! that changes it is a movement ([`crate::inventory_moves`]). A quantity
//! edited in place cannot answer the one question a warehouse actually asks —
//! "where did the other four go?".
//!
//! Two defaults this file chooses, both the stock screen's own question rather
//! than the ledger's:
//!
//! - **The virtual counterparties are left out** unless `includeVirtual=1`.
//!   `supplier` holding minus four hundred is an accounting fact about how much
//!   has come from outside, not a shelf, and putting it on a stock screen makes
//!   every total on that screen wrong.
//! - **Rows that have fallen back to zero are left out** unless
//!   `includeZero=1`. A product that came and went is not stock — but it is
//!   exactly what one product's own history page wants to show.
//!
//! Money is integer cents throughout, valued at the product's purchase price by
//! the store's one valuation function; the client never multiplies anything.

use axum::Json;
use axum::extract::{Query, State};
use axum::http::HeaderMap;
use serde::Deserialize;
use serde_json::{Value, json};

use alo_store::inv_stock::{StockFilter, StockLevel};
use alo_store::{BillingProductId, InvLocationId};

use crate::billing::{flag, iso, map_store_err};
use crate::error::Problem;
use crate::state::{AppState, authenticate};

/// One product at one place as JSON. `qtyMilli` is signed — negative is
/// legitimate on a virtual counterparty and impossible on a real location.
///
/// Shared with [`crate::inventory_scan`], which answers a scanned code with
/// this same row: "how many are there, and where" is one shape whichever
/// question asked it.
pub(crate) fn level_json(level: &StockLevel) -> Value {
    json!({
        "productId": level.product_id.as_str(),
        "productName": level.product_name,
        "sku": level.sku,
        "locationId": level.location_id.as_str(),
        "locationCode": level.location_code,
        "locationName": level.location_name,
        "locationKind": level.location_kind.as_str(),
        "real": level.location_kind.is_real(),
        "qtyMilli": level.qty_milli,
        "valueCents": level.value_cents,
        "lastMoveAt": iso(level.last_move_at),
    })
}

/// The stock read's query string. Every field narrows; all of them absent is
/// "what is on the shelves, right now".
#[derive(Deserialize)]
pub struct StockQuery {
    /// One product across every location.
    #[serde(default, rename = "productId")]
    product_id: Option<String>,
    /// One location across every product.
    #[serde(default, rename = "locationId")]
    location_id: Option<String>,
    /// `includeVirtual=1` adds the four counterparties.
    #[serde(default, rename = "includeVirtual")]
    include_virtual: Option<String>,
    /// `includeZero=1` keeps the rows that have fallen back to zero.
    #[serde(default, rename = "includeZero")]
    include_zero: Option<String>,
}

/// `GET /inventory/stock[?productId&locationId&includeVirtual=1&includeZero=1]`
/// → `{"stock":[…],"totalValueCents":n}`.
///
/// The total is the sum of what is listed — so a caller who asked for one
/// warehouse gets that warehouse's value, and one who added the virtual
/// counterparties gets a number that sums to roughly zero, which is the correct
/// reading of a closed ledger rather than a bug.
pub async fn list_stock(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(q): Query<StockQuery>,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let filter = StockFilter {
        product_id: q.product_id.map(BillingProductId::new),
        location_id: q.location_id.map(InvLocationId::new),
        include_virtual: flag(q.include_virtual.as_deref()),
        include_zero: flag(q.include_zero.as_deref()),
    };
    let levels = account
        .acc
        .inv_stock(&filter)
        .await
        .map_err(map_store_err)?;
    let total: i64 = levels.iter().map(|level| level.value_cents).sum();
    Ok(Json(json!({
        "stock": levels.iter().map(level_json).collect::<Vec<_>>(),
        "totalValueCents": total,
    })))
}

#[cfg(test)]
mod tests {
    use super::*;
    use alo_store::inv_locations::LocationKind;
    use alo_store::inv_stock::stock_value_cents;
    use time::OffsetDateTime;

    fn level(qty_milli: i64, kind: LocationKind) -> StockLevel {
        StockLevel {
            product_id: BillingProductId::new("p1".to_owned()),
            product_name: "Blue chair".to_owned(),
            sku: "CH-1".to_owned(),
            location_id: InvLocationId::new("l1".to_owned()),
            location_code: "MAIN".to_owned(),
            location_name: "Hoofdmagazijn".to_owned(),
            location_kind: kind,
            qty_milli,
            value_cents: stock_value_cents(qty_milli, 2_150),
            last_move_at: OffsetDateTime::now_utc(),
        }
    }

    #[test]
    fn a_row_says_where_it_is_and_whether_that_is_a_real_place() {
        let json = level_json(&level(12_000, LocationKind::Stock));
        assert_eq!(json["qtyMilli"], 12_000);
        assert_eq!(json["valueCents"], 25_800);
        assert_eq!(json["locationKind"], "stock");
        assert_eq!(json["real"], true);

        let virtual_row = level_json(&level(-12_000, LocationKind::Supplier));
        assert_eq!(virtual_row["qtyMilli"], -12_000);
        assert_eq!(
            virtual_row["valueCents"], -25_800,
            "a counterparty's value is the exact mirror, never an absolute"
        );
        assert_eq!(virtual_row["real"], false);
    }
}
