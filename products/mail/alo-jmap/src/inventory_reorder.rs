//! Reorder rules and the shortage report over HTTP (alo Inventory, ADR 0035,
//! wave B5.07) — over [`alo_store::inv_reorder`].
//!
//! Two surfaces with opposite characters, deliberately in one file because the
//! second is nothing but the fold of the first:
//!
//! - **`/inventory/reorder-rules`** is an ordinary CRUD collection: the minima
//!   a tenant types. `PATCH` merges onto the stored rule, so nudging a target
//!   cannot silently park the rule because a stale form carried the flag — the
//!   convention [`crate::inventory_locations`] set for the same reason.
//!   The pair a rule watches is **not** patchable: a rule about a different
//!   product at a different shelf is a different rule.
//! - **`/inventory/shortages`** has no writable surface and never will. It is
//!   derived — on-hand from the ledger, on-order from the placed purchase
//!   orders, committed from the confirmed sales orders — and the one door that
//!   changes any of those three is the document that caused it.
//!
//! **The client never does the arithmetic.** Every row states `onHandQtyMilli`,
//! `onOrderQtyMilli`, `committedQtyMilli`, the `availableQtyMilli` they add up
//! to, how far under the minimum that is, what to buy and what it would cost —
//! all computed server-side, because a screen that subtracts its own numbers
//! and an agent that proposes an order from the same route must not be able to
//! disagree about how short we are.
//!
//! The CSV twin is `/inventory/shortages.csv`, a URL that names its format
//! rather than a `?format=`, exactly as the reports before it (B1.20). Its
//! column names are a contract and deliberately untranslated; a screen shows
//! the caller's language, a file feeds a spreadsheet.

use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::Response;
use serde::Deserialize;
use serde_json::{Value, json};

use alo_store::AccountStore;
use alo_store::inv_reorder::{
    NewReorderRule, ReorderLimits, ReorderRule, ReorderRuleFilter, Shortage, ShortageFilter,
};
use alo_store::{BillingProductId, InvLocationId, InvReorderRuleId, InvSupplierId};

use crate::billing::{flag, iso, map_store_err, parse_body};
use crate::csv;
use crate::error::Problem;
use crate::state::{AppState, authenticate};

/// A rule as JSON. The ends carry their names as well as their ids: a rule
/// showing two opaque strings is a rule nobody can check.
fn rule_json(r: &ReorderRule) -> Value {
    json!({
        "id": r.id.as_str(),
        "productId": r.product_id.as_str(),
        "productName": r.product_name,
        "sku": r.sku,
        "unit": r.unit,
        "locationId": r.location_id.as_str(),
        "locationCode": r.location_code,
        "locationName": r.location_name,
        "minQtyMilli": r.min_qty_milli,
        "targetQtyMilli": r.target_qty_milli,
        "active": r.active,
        "createdBy": r.created_by,
        "createdAt": iso(r.created_at),
        "updatedAt": iso(r.updated_at),
    })
}

/// One shortage as JSON: the four inputs, the three numbers they imply, and the
/// supplier a proposal would be written against.
pub(crate) fn shortage_json(s: &Shortage) -> Value {
    json!({
        "ruleId": s.rule_id.as_str(),
        "productId": s.product_id.as_str(),
        "productName": s.product_name,
        "sku": s.sku,
        "unit": s.unit,
        "locationId": s.location_id.as_str(),
        "locationCode": s.location_code,
        "locationName": s.location_name,
        "minQtyMilli": s.min_qty_milli,
        "targetQtyMilli": s.target_qty_milli,
        "onHandQtyMilli": s.on_hand_qty_milli,
        "onOrderQtyMilli": s.on_order_qty_milli,
        "committedQtyMilli": s.committed_qty_milli,
        "availableQtyMilli": s.available_qty_milli,
        "shortByQtyMilli": s.short_by_qty_milli,
        "buyQtyMilli": s.buy_qty_milli,
        "estimatedCostCents": s.estimated_cost_cents,
        "supplier": s.supplier.as_ref().map(|sup| json!({
            "supplierId": sup.supplier_id.as_str(),
            "supplierName": sup.supplier_name,
            "supplierCode": sup.supplier_code,
            "purchasePriceCents": sup.purchase_price_cents,
            "currency": sup.currency,
            "minOrderQtyMilli": sup.min_order_qty_milli,
            "leadTimeDays": sup.lead_time_days,
        })),
    })
}

/// The writable fields of a rule.
///
/// The pair is accepted on create and **ignored on update**: the product and
/// the location are what the rule is *about*, and re-pointing one in place would
/// silently rewrite what a screen was looking at. The store refuses to change
/// them at all; this layer simply never offers it the chance.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RuleBody {
    #[serde(default)]
    product_id: Option<String>,
    #[serde(default)]
    location_id: Option<String>,
    #[serde(default)]
    min_qty_milli: Option<i64>,
    #[serde(default)]
    target_qty_milli: Option<i64>,
    #[serde(default)]
    active: Option<bool>,
}

impl RuleBody {
    /// The whole rule this body states. Both ends are required — a rule without
    /// a product or a place is not an instruction.
    fn into_rule(self) -> Result<NewReorderRule, Problem> {
        let product_id = self
            .product_id
            .filter(|s| !s.trim().is_empty())
            .ok_or_else(|| {
                Problem::with(
                    StatusCode::UNPROCESSABLE_ENTITY,
                    "productId is required: a reorder rule is about a product",
                )
            })?;
        let location_id = self
            .location_id
            .filter(|s| !s.trim().is_empty())
            .ok_or_else(|| {
                Problem::with(
                    StatusCode::UNPROCESSABLE_ENTITY,
                    "locationId is required: a reorder rule is about a place",
                )
            })?;
        Ok(NewReorderRule {
            product_id: BillingProductId::new(product_id),
            location_id: InvLocationId::new(location_id),
            // A rule with no numbers watches for "less than nothing", which
            // never comes true — harmless, and honest about what was sent.
            min_qty_milli: self.min_qty_milli.unwrap_or(0),
            target_qty_milli: self.target_qty_milli.unwrap_or(0),
            // Written to be watched unless the caller parks it up front.
            active: self.active.unwrap_or(true),
        })
    }

    /// Merges the stated numbers onto the stored rule, leaving the rest as they
    /// were.
    fn onto(self, stored: &ReorderRule) -> ReorderLimits {
        ReorderLimits {
            min_qty_milli: self.min_qty_milli.unwrap_or(stored.min_qty_milli),
            target_qty_milli: self.target_qty_milli.unwrap_or(stored.target_qty_milli),
            active: self.active.unwrap_or(stored.active),
        }
    }
}

/// Loads one of the tenant's rules, or fails with the `404` an id from another
/// tenant gets.
async fn load(acc: &AccountStore, id: &InvReorderRuleId) -> Result<ReorderRule, Problem> {
    acc.inv_reorder_rule(id)
        .await
        .map_err(map_store_err)?
        .ok_or_else(|| Problem::with(StatusCode::NOT_FOUND, "no such reorder rule"))
}

/// Query string of the rules list.
#[derive(Deserialize)]
pub struct RuleListQuery {
    /// One product across every place it is watched.
    #[serde(default, rename = "productId")]
    product_id: Option<String>,
    /// One place across every product watched there.
    #[serde(default, rename = "locationId")]
    location_id: Option<String>,
    /// `includeInactive=1` also returns the parked rules.
    #[serde(default, rename = "includeInactive")]
    include_inactive: Option<String>,
}

/// `GET /inventory/reorder-rules[?productId&locationId&includeInactive=1]` →
/// `{"rules":[…]}` — what the tenant is watching, in product-name order.
pub async fn list_rules(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(q): Query<RuleListQuery>,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let filter = ReorderRuleFilter {
        product_id: q.product_id.map(BillingProductId::new),
        location_id: q.location_id.map(InvLocationId::new),
        include_inactive: flag(q.include_inactive.as_deref()),
    };
    let rules = account
        .acc
        .inv_reorder_rules(&filter)
        .await
        .map_err(map_store_err)?;
    Ok(Json(json!({
        "rules": rules.iter().map(rule_json).collect::<Vec<_>>(),
    })))
}

/// `POST /inventory/reorder-rules` `{productId, locationId, minQtyMilli,
/// targetQtyMilli, active?}` → `{"rule":{…}}`.
///
/// A `422` on a service product, a location that is not a real shelf, or a
/// target under the minimum; a `409` when the pair is already watched, because
/// two minima for one shelf are two answers to one question.
pub async fn create_rule(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: axum::body::Bytes,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let req: RuleBody = parse_body(&body)?;
    let id = account
        .acc
        .create_inv_reorder_rule(&req.into_rule()?)
        .await
        .map_err(map_store_err)?;
    let rule = load(&account.acc, &id).await?;
    Ok(Json(json!({ "rule": rule_json(&rule) })))
}

/// `GET /inventory/reorder-rules/{id}` → `{"rule":{…}}`.
pub async fn get_rule(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let rule = load(&account.acc, &InvReorderRuleId::new(id)).await?;
    Ok(Json(json!({ "rule": rule_json(&rule) })))
}

/// `PATCH /inventory/reorder-rules/{id}` `{minQtyMilli?, targetQtyMilli?,
/// active?}` → `{"rule":{…}}` — change the numbers, or park the rule.
pub async fn update_rule(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
    body: axum::body::Bytes,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let req: RuleBody = parse_body(&body)?;
    let id = InvReorderRuleId::new(id);
    let stored = load(&account.acc, &id).await?;
    account
        .acc
        .update_inv_reorder_rule(&id, &req.onto(&stored))
        .await
        .map_err(map_store_err)?;
    let rule = load(&account.acc, &id).await?;
    Ok(Json(json!({ "rule": rule_json(&rule) })))
}

/// `DELETE /inventory/reorder-rules/{id}` → `{"removed":true}` — stop watching
/// the pair altogether.
///
/// Deleted rather than archived, and safe in a way deleting a location is not: a
/// rule explains nothing that happened, and no document copied anything from it.
/// A tenant who wants the numbers kept parks the rule instead.
pub async fn delete_rule(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    account
        .acc
        .delete_inv_reorder_rule(&InvReorderRuleId::new(id))
        .await
        .map_err(map_store_err)?;
    Ok(Json(json!({ "removed": true })))
}

/// Query string of the shortage report, shared by the screen and the file.
#[derive(Deserialize)]
pub struct ShortageQuery {
    /// One place — the question a single warehouse's buyer asks.
    #[serde(default, rename = "locationId")]
    location_id: Option<String>,
    /// One product, across every place it is watched.
    #[serde(default, rename = "productId")]
    product_id: Option<String>,
    /// Only what this supplier sells us — the slice one proposed order needs.
    #[serde(default, rename = "supplierId")]
    supplier_id: Option<String>,
}

impl ShortageQuery {
    fn filter(self) -> ShortageFilter {
        ShortageFilter {
            location_id: self.location_id.map(InvLocationId::new),
            product_id: self.product_id.map(BillingProductId::new),
            supplier_id: self.supplier_id.map(InvSupplierId::new),
        }
    }
}

/// Reads the report behind both routes — one gate, one store call, so the file
/// a buyer saves and the table on their screen cannot disagree.
async fn read(
    state: &AppState,
    headers: &HeaderMap,
    query: ShortageQuery,
) -> Result<Vec<Shortage>, Problem> {
    let account = authenticate(state, headers).await?;
    account
        .acc
        .inv_shortages(&query.filter())
        .await
        .map_err(map_store_err)
}

/// `GET /inventory/shortages[?locationId&productId&supplierId]` →
/// `{"shortages":[…],"count":n}` — what needs buying, how much, and from whom.
///
/// There is deliberately **no grand total**: each row's cost is quoted in the
/// currency its supplier quotes in, and adding francs to euro to reach one
/// number would be a conversion nobody asked for. A screen that wants a total
/// per currency has every row it needs to make one.
pub async fn list_shortages(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(q): Query<ShortageQuery>,
) -> Result<Json<Value>, Problem> {
    let shortages = read(&state, &headers, q).await?;
    Ok(Json(json!({
        "shortages": shortages.iter().map(shortage_json).collect::<Vec<_>>(),
        "count": shortages.len(),
    })))
}

/// The CSV column names — a contract, deliberately not translated.
const COLUMNS: [&str; 17] = [
    "product",
    "sku",
    "unit",
    "location",
    "min",
    "target",
    "on_hand",
    "on_order",
    "committed",
    "available",
    "short_by",
    "to_buy",
    "supplier",
    "supplier_code",
    "unit_price",
    "currency",
    "estimated_cost",
];

/// A quantity in milli-units as a machine-readable decimal: `1500` → `1.5`,
/// `2000` → `2`. A file is read by a spreadsheet, so the separator is a point
/// and there is no thousands grouping, whatever the caller's language is.
fn quantity(qty_milli: i64) -> String {
    let sign = if qty_milli < 0 { "-" } else { "" };
    let magnitude = i128::from(qty_milli).unsigned_abs();
    let thousandths = magnitude % 1_000;
    if thousandths == 0 {
        return format!("{sign}{}", magnitude / 1_000);
    }
    let fraction = format!("{thousandths:03}");
    format!(
        "{sign}{}.{}",
        magnitude / 1_000,
        fraction.trim_end_matches('0')
    )
}

/// An amount in integer cents as a machine-readable decimal (`63000` →
/// `630.00`). The absolute value is taken in `i128` so `i64::MIN` prints rather
/// than panicking.
fn amount(cents: i64) -> String {
    let sign = if cents < 0 { "-" } else { "" };
    let abs = i128::from(cents).abs();
    format!("{sign}{}.{:02}", abs / 100, abs % 100)
}

/// The report as one table: a header row and a row per shortage, in the order
/// the screen shows them.
///
/// The location is written as `CODE — Name`, which is what a person reading the
/// file needs; the ids are not in the file at all, because a spreadsheet is read
/// by a human and a machine that wants ids has the JSON.
fn report_csv(shortages: &[Shortage]) -> String {
    let mut out = csv::row(&COLUMNS);
    for s in shortages {
        let supplier = s.supplier.as_ref();
        out.push_str(&csv::row(&[
            &s.product_name,
            &s.sku,
            &s.unit,
            &format!("{} \u{2014} {}", s.location_code, s.location_name),
            &quantity(s.min_qty_milli),
            &quantity(s.target_qty_milli),
            &quantity(s.on_hand_qty_milli),
            &quantity(s.on_order_qty_milli),
            &quantity(s.committed_qty_milli),
            &quantity(s.available_qty_milli),
            &quantity(s.short_by_qty_milli),
            &quantity(s.buy_qty_milli),
            supplier.map_or("", |sup| sup.supplier_name.as_str()),
            supplier.map_or("", |sup| sup.supplier_code.as_str()),
            &supplier.map_or_else(String::new, |sup| amount(sup.purchase_price_cents)),
            supplier.map_or("", |sup| sup.currency.as_str()),
            &amount(s.estimated_cost_cents),
        ]));
    }
    out
}

/// `GET /inventory/shortages.csv[?locationId&productId&supplierId]` → the same
/// rows as a file a buyer can hand to somebody.
pub async fn shortages_csv(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(q): Query<ShortageQuery>,
) -> Result<Response, Problem> {
    let shortages = read(&state, &headers, q).await?;
    Ok(csv::attachment(report_csv(&shortages), "shortages.csv"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use alo_store::inv_reorder::ShortageSupplier;
    use time::OffsetDateTime;

    fn body(value: Value) -> RuleBody {
        serde_json::from_value(value).unwrap_or_else(|e| panic!("body rejected: {e}"))
    }

    fn stored() -> ReorderRule {
        ReorderRule {
            id: InvReorderRuleId::new("r1"),
            product_id: BillingProductId::new("p1"),
            product_name: "Blue chair".to_owned(),
            sku: "CH-1".to_owned(),
            unit: "piece".to_owned(),
            location_id: InvLocationId::new("l1"),
            location_code: "MAIN".to_owned(),
            location_name: "Hoofdmagazijn".to_owned(),
            min_qty_milli: 4_000,
            target_qty_milli: 20_000,
            active: true,
            created_by: "u".to_owned(),
            created_at: OffsetDateTime::UNIX_EPOCH,
            updated_at: OffsetDateTime::UNIX_EPOCH,
        }
    }

    fn shortage(supplier: Option<ShortageSupplier>) -> Shortage {
        Shortage {
            rule_id: InvReorderRuleId::new("r1"),
            product_id: BillingProductId::new("p1"),
            product_name: "Blue chair".to_owned(),
            sku: "CH-1".to_owned(),
            unit: "piece".to_owned(),
            location_id: InvLocationId::new("l1"),
            location_code: "MAIN".to_owned(),
            location_name: "Hoofdmagazijn".to_owned(),
            min_qty_milli: 4_000,
            target_qty_milli: 20_000,
            on_hand_qty_milli: 1_500,
            on_order_qty_milli: 0,
            committed_qty_milli: 500,
            available_qty_milli: 1_000,
            short_by_qty_milli: 3_000,
            buy_qty_milli: 19_000,
            supplier,
            estimated_cost_cents: 59_850,
        }
    }

    fn hoffmann() -> ShortageSupplier {
        ShortageSupplier {
            supplier_id: InvSupplierId::new("s1"),
            supplier_name: "Hoffmann, GmbH".to_owned(),
            supplier_code: "HM-4471".to_owned(),
            purchase_price_cents: 3_150,
            currency: "EUR".to_owned(),
            min_order_qty_milli: 10_000,
            lead_time_days: 9,
        }
    }

    #[test]
    fn a_rule_needs_both_of_its_ends() {
        assert!(body(json!({ "locationId": "l1" })).into_rule().is_err());
        assert!(body(json!({ "productId": "p1" })).into_rule().is_err());
        // A blank string is an absent id, not an id that happens to be empty.
        assert!(
            body(json!({ "productId": "  ", "locationId": "l1" }))
                .into_rule()
                .is_err()
        );
        let stated = body(json!({ "productId": "p1", "locationId": "l1" }))
            .into_rule()
            .unwrap_or_else(|e| panic!("rejected: {e:?}"));
        assert_eq!(stated.min_qty_milli, 0);
        assert_eq!(stated.target_qty_milli, 0);
        assert!(stated.active, "a new rule is written to be watched");
    }

    #[test]
    fn a_patch_leaves_alone_what_it_does_not_state() {
        // Nudging the target cannot park the rule, and cannot move the minimum.
        let merged = body(json!({ "targetQtyMilli": 30_000 })).onto(&stored());
        assert_eq!(merged.min_qty_milli, 4_000);
        assert_eq!(merged.target_qty_milli, 30_000);
        assert!(merged.active);
        // …and parking it cannot move either number.
        let parked = body(json!({ "active": false })).onto(&stored());
        assert_eq!(parked.min_qty_milli, 4_000);
        assert_eq!(parked.target_qty_milli, 20_000);
        assert!(!parked.active);
        // An empty patch is a no-op, not a reset.
        let untouched = body(json!({})).onto(&stored());
        assert_eq!(untouched.min_qty_milli, 4_000);
        assert_eq!(untouched.target_qty_milli, 20_000);
        assert!(untouched.active);
    }

    #[test]
    fn the_pair_is_not_patchable() {
        // The store cannot re-point a rule, and this layer never asks it to:
        // the merged value carries only the three fields that can change.
        let merged =
            body(json!({ "productId": "other", "locationId": "elsewhere" })).onto(&stored());
        assert_eq!(merged.min_qty_milli, 4_000);
        assert_eq!(merged.target_qty_milli, 20_000);
    }

    #[test]
    fn quantities_are_integers_on_the_wire() {
        // 1.5 units is 1500 milli-units; a client that sends 1.5 gets a 400,
        // not a silently rounded quantity.
        assert!(serde_json::from_value::<RuleBody>(json!({"minQtyMilli": 1.5})).is_err());
        assert!(serde_json::from_value::<RuleBody>(json!({"targetQtyMilli": "20"})).is_err());
        assert!(serde_json::from_value::<RuleBody>(json!({"active": "yes"})).is_err());
    }

    #[test]
    fn a_shortage_states_every_number_it_added_up() {
        let rendered = shortage_json(&shortage(Some(hoffmann())));
        assert_eq!(rendered["onHandQtyMilli"], 1_500);
        assert_eq!(rendered["onOrderQtyMilli"], 0);
        assert_eq!(rendered["committedQtyMilli"], 500);
        assert_eq!(
            rendered["availableQtyMilli"], 1_000,
            "the client is told the sum, never left to compute it"
        );
        assert_eq!(rendered["shortByQtyMilli"], 3_000);
        assert_eq!(rendered["buyQtyMilli"], 19_000);
        assert_eq!(rendered["estimatedCostCents"], 59_850);
        assert_eq!(rendered["supplier"]["supplierName"], "Hoffmann, GmbH");
        assert_eq!(rendered["supplier"]["leadTimeDays"], 9);
    }

    #[test]
    fn a_shortage_nobody_quotes_for_names_no_supplier() {
        let rendered = shortage_json(&shortage(None));
        assert_eq!(
            rendered["supplier"],
            Value::Null,
            "a product no supplier sells us is still short; it just has nobody to buy it from"
        );
        assert_eq!(rendered["buyQtyMilli"], 19_000);
    }

    #[test]
    fn the_file_is_one_table_of_shortage_rows() {
        let body = report_csv(&[shortage(Some(hoffmann()))]);
        let mut lines = body.split_terminator("\r\n");
        assert_eq!(
            lines.next(),
            Some(COLUMNS.join(",").as_str()),
            "the header is the contract"
        );
        let row = lines.next().unwrap_or_default();
        assert!(row.starts_with("Blue chair,CH-1,piece,MAIN \u{2014} Hoofdmagazijn,"));
        assert!(row.contains(",4,20,1.5,0,0.5,1,3,19,"));
        assert!(
            row.contains("\"Hoffmann, GmbH\""),
            "a supplier name with a comma is quoted, not truncated: {row}"
        );
        assert!(row.ends_with(",HM-4471,31.50,EUR,598.50"));
        assert_eq!(lines.next(), None, "one shortage, one row");
    }

    #[test]
    fn a_row_with_no_supplier_leaves_those_columns_empty() {
        let body = report_csv(&[shortage(None)]);
        let row = body
            .split_terminator("\r\n")
            .nth(1)
            .unwrap_or_default()
            .to_owned();
        assert!(
            row.ends_with(",19,,,,,598.50"),
            "four empty supplier columns, and the estimate still stands: {row}"
        );
    }

    #[test]
    fn the_empty_report_is_a_header_and_nothing_else() {
        assert_eq!(report_csv(&[]), csv::row(&COLUMNS));
    }

    #[test]
    fn quantities_and_amounts_read_the_same_in_every_language() {
        // A file feeds a spreadsheet: a point, no grouping, no minus sign that
        // is not ASCII.
        assert_eq!(quantity(2_000), "2");
        assert_eq!(quantity(1_500), "1.5");
        assert_eq!(quantity(1_001), "1.001");
        assert_eq!(quantity(-2_500), "-2.5");
        assert_eq!(quantity(0), "0");
        assert_eq!(quantity(500), "0.5");
        assert_eq!(amount(63_000), "630.00");
        assert_eq!(amount(-5), "-0.05");
        assert_eq!(amount(0), "0.00");
    }
}
