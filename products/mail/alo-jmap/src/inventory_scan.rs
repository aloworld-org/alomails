//! Scanning over HTTP (alo Inventory, ADR 0035, wave B5.09c) — the one call
//! behind "point the thing at the box".
//!
//! A warehouse identifies an item with a machine rather than with its eyes, and
//! whichever machine reads the code — a keyboard-wedge scanner that types
//! digits and an Enter, or a phone camera — the question it asks is the same:
//! *which of our products is this, and how much of it is there*. That is why
//! there is one route and not two (`docs/design/inventory.md` § Web surface):
//! the camera is an input device, not a feature with a surface of its own.
//!
//! Three refusals, and the difference between them is the point of the route:
//!
//! - **A code that is not a GTIN is a `422`**, carrying the barcode module's
//!   own sentence — *a barcode must be 8, 12, 13 or 14 digits*, *the check
//!   digit does not match*. A misread scan and an unknown product are
//!   different facts, and a person who is told "not found" for a code the
//!   scanner mangled will hunt for a product that was there all along.
//! - **A well-formed code nobody's catalog carries is a `404`.** Existence is
//!   never disclosed across tenants: another tenant's barcode answers exactly
//!   the same way as a code nobody has.
//! - **No code at all is a `422`**, not an empty answer. `?code=` with nothing
//!   after it is a client bug, and answering it with "no product" would send
//!   somebody looking for the label instead of for the wiring.
//!
//! The answer carries the product *and* its on-hand by real location, because
//! the two reads always happen together — a scan that returned a name and made
//! the screen ask a second question would flicker on the phone that scanned it.
//! The quantity total is the server's, over integer milli-units, as everywhere
//! else in this module: the client adds nothing up.

use axum::Json;
use axum::extract::{Query, State};
use axum::http::{HeaderMap, StatusCode};
use serde::Deserialize;
use serde_json::{Value, json};

use alo_store::inv_barcode;
use alo_store::inv_stock::StockFilter;

use crate::billing::map_store_err;
use crate::billing_products::product_json;
use crate::error::Problem;
use crate::inventory_stock::level_json;
use crate::state::{AppState, authenticate};

/// The scan's query string.
#[derive(Deserialize)]
pub struct ScanQuery {
    /// The code as the scanner read it. Spaces and hyphens are presentation
    /// and are removed before the check digit is tested, so a code read off a
    /// box in groups is the same code.
    #[serde(default)]
    code: Option<String>,
}

/// Validates the scanned code, or says why it cannot be one.
///
/// Blank is an error here although [`inv_barcode::canonicalize`] calls it
/// `Ok(None)`: a *stored* product with no barcode is the normal case, and a
/// *scan* of nothing is not a scan.
fn scanned(raw: Option<&str>) -> Result<String, Problem> {
    let stated = raw.unwrap_or_default();
    match inv_barcode::canonicalize(stated) {
        Ok(Some(code)) => Ok(code),
        Ok(None) => Err(Problem::with(
            StatusCode::UNPROCESSABLE_ENTITY,
            "code is required: a scan carries the code that was read",
        )),
        // The store's own sentence, which names the rule and never the code —
        // a barcode is a fact about a tenant's stock and does not go into a
        // log line or an error body.
        Err(refusal) => Err(Problem::with(
            StatusCode::UNPROCESSABLE_ENTITY,
            refusal.to_string(),
        )),
    }
}

/// `GET /inventory/scan?code=…` → `{"code":"…","product":{…},"stock":[…],
/// "onHandQtyMilli":n}` — what was scanned, what it is, and where it is.
///
/// `stock` is the real locations only: a scan asks what is on a shelf, and the
/// virtual counterparties are an accounting fact about a closed ledger rather
/// than a place a person can walk to. Rows that have fallen back to zero are
/// left out by the same read, so a product that came and went scans as itself
/// with nothing on hand — which is `onHandQtyMilli: 0` and an empty list, not a
/// missing product.
pub async fn scan(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(q): Query<ScanQuery>,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let code = scanned(q.code.as_deref())?;
    let product = account
        .acc
        .billing_product_by_barcode(&code)
        .await
        .map_err(map_store_err)?
        .ok_or_else(|| {
            Problem::with(
                StatusCode::NOT_FOUND,
                "no product in this catalog carries this barcode",
            )
        })?;
    // A service has no shelf: the ledger refuses to move one, so the read is
    // skipped rather than answered with an empty list that reads as "none
    // left". The client shows the product and no quantity at all.
    let levels = if product.stocked {
        account
            .acc
            .inv_stock(&StockFilter {
                product_id: Some(product.id.clone()),
                location_id: None,
                include_virtual: false,
                include_zero: false,
            })
            .await
            .map_err(map_store_err)?
    } else {
        Vec::new()
    };
    let on_hand: i64 = levels.iter().map(|level| level.qty_milli).sum();
    Ok(Json(json!({
        "code": code,
        "product": product_json(&product),
        "stock": levels.iter().map(level_json).collect::<Vec<_>>(),
        "onHandQtyMilli": on_hand,
    })))
}

#[cfg(test)]
#[allow(clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn separators_are_presentation_and_the_code_is_digits() {
        assert_eq!(
            scanned(Some(" 400-638 133 393 1 ")).expect("a real EAN-13"),
            "4006381333931"
        );
    }

    /// The refusal's sentence, which is the whole of what a scanning person
    /// sees: every one of these routes' `422`s is shown verbatim.
    fn refusal(stated: Option<&str>) -> String {
        let refused = scanned(stated).expect_err("this code cannot be scanned");
        assert_eq!(refused.status, StatusCode::UNPROCESSABLE_ENTITY);
        refused.detail.unwrap_or_default()
    }

    #[test]
    fn a_misread_code_is_a_422_that_names_the_rule_and_not_the_code() {
        let wrong_digit = refusal(Some("4006381333930"));
        assert!(
            wrong_digit.contains("check digit"),
            "a scanner's user needs to know to scan again: {wrong_digit}"
        );
        assert!(
            !wrong_digit.contains("4006381333930"),
            "a barcode is a fact about a tenant's stock and never travels in an error"
        );

        let short = refusal(Some("40063"));
        assert!(short.contains("8, 12, 13 or 14"), "{short}");

        let letters = refusal(Some("CH-1"));
        assert!(letters.contains("digits"), "{letters}");
    }

    #[test]
    fn a_scan_of_nothing_is_a_client_bug_and_says_so() {
        for stated in [None, Some(""), Some("   ")] {
            let empty = refusal(stated);
            assert!(empty.contains("code is required"), "{empty}");
        }
    }
}
