//! Executing the **Inventory** tools of an approved agent proposal (ADR 0034,
//! ADR 0035 wave B5.10) — the acting half of what [`alo_ai::agent_inventory`]
//! describes to the model.
//!
//! Called only from [`crate::agent::agent_execute`], which is the single acting
//! path: the user saw the proposal and approved it. Everything here runs
//! through the caller's own tenant-scoped store handle, so an agent can no more
//! reach another tenant's shelf, supplier or order than the browser that asked
//! it can.
//!
//! Four rules shape this module, and they are why it is not thin glue:
//!
//! - **Nothing here decides what to buy.** The quantities are the shortage
//!   query's ([`alo_store::AccountStore::inv_shortages`]): the tenant's own
//!   minimums against their own shelves, their own open orders and their own
//!   promises. The prices are the offer their supplier already quoted them. This
//!   executor parses at most two *narrowings* — one supplier, one place — and
//!   nothing else, because a purchase order is a document that goes to another
//!   company and a number on it that nobody can trace is the one mistake this
//!   surface must not be able to make.
//! - **What it writes is a draft, all the way down.** A draft carries no number
//!   and has been sent to nobody; drawing the number and posting the covering
//!   mail is `POST /inventory/purchase-orders/{id}/send`, which a person
//!   presses (B5.05a2, `docs/design/inventory.md` § The inventory agent).
//! - **One document never mixes two currencies.** Drafts are grouped by supplier
//!   *and* by the currency their offer is quoted in. A supplier who quotes us in
//!   two currencies is rare and is two drafts — never one order whose lines add
//!   up to a total in no currency at all.
//! - **What was left out is part of the answer.** A shortage nobody has quoted
//!   us for is not ordered from a supplier picked here; it comes back as a
//!   skipped line with a machine-readable reason the client writes words for.
//!   Silence would read as "everything is on order", which is a different and
//!   usually wrong statement.
//!
//! The results carry figures and reason codes only — never a sentence. A
//! sentence composed here would be a user-facing string authored in the server
//! in one language, which is a bug in a European product (CLAUDE.md).

use std::collections::HashMap;

use axum::Json;
use serde_json::{Value, json};
use time::OffsetDateTime;

use alo_store::inv_locations::{Location, LocationKind};
use alo_store::inv_po::NewPurchaseOrder;
use alo_store::inv_po_lines::NewPoLine;
use alo_store::inv_reorder::{
    ProductPipeline, ReorderRule, ReorderRuleFilter, Shortage, ShortageFilter, available_qty_milli,
};
use alo_store::inv_stock::{StockFilter, StockLevel};
use alo_store::{BillingProductId, InvPurchaseOrderId, InvSupplierId, NewLine, Product};

use crate::agent_args::{pick, string_arg, unprocessable};
use crate::billing::map_store_err;
use crate::billing_products::product_json;
use crate::error::Problem;
use crate::inventory_po::document_json;
use crate::inventory_stock::level_json;
use crate::state::Account;

/// `reorder_proposals` — one **draft** purchase order per supplier for
/// everything the caller is under their own minimum on.
///
/// The order is: read the two narrowings, ask the store what is short, group
/// what has an offer behind it, then write one draft per group and read each
/// back for its totals. Nothing is written until every name in the proposal has
/// resolved, so a proposal naming a supplier that does not exist leaves no
/// half-made document behind.
///
/// # Errors
/// `422` when a stated supplier or place cannot be resolved to exactly one
/// record; the store's own `422`/`409`/`500` otherwise.
pub async fn execute_reorder_proposals(
    account: &Account,
    args: &Value,
) -> Result<Json<Value>, Problem> {
    let supplier = resolve_supplier(account, args).await?;
    let location = resolve_location(account, args).await?;
    let shortages = account
        .acc
        .inv_shortages(&ShortageFilter {
            location_id: location.as_ref().map(|place| place.id.clone()),
            product_id: None,
            supplier_id: supplier.as_ref().map(|one| one.0.clone()),
        })
        .await
        .map_err(map_store_err)?;

    // The catalog, once: a line needs the VAT rate the product carries, and the
    // shortage row carries the buying facts rather than the tax ones. Archived
    // products are already out of the shortage query, so the active read is the
    // whole of what can be ordered.
    let products = account
        .acc
        .billing_products(false)
        .await
        .map_err(map_store_err)?;
    let rates: HashMap<&str, &Product> = products
        .iter()
        .map(|product| (product.id.as_str(), product))
        .collect();

    let plan = plan_orders(&shortages, &rates);
    let mut drafted = Vec::with_capacity(plan.groups.len());
    for group in &plan.groups {
        let id = write_draft(account, group).await?;
        drafted.push(draft_json(account, &id, &group.lines).await?);
    }

    Ok(Json(json!({
        "ok": true,
        "result": {
            "kind": "reorderProposals",
            // What the caller narrowed to, echoed so the card can say what it
            // looked at rather than implying it looked at everything.
            "supplier": supplier.map(|(id, name)| json!({
                "supplierId": id.as_str(), "supplierName": name,
            })),
            "location": location.map(|place| json!({
                "locationId": place.id.as_str(),
                "locationCode": place.code,
                "locationName": place.name,
            })),
            "drafted": drafted,
            "skipped": plan
                .skipped
                .iter()
                .map(|skipped| json!({
                    "productId": skipped.product_id.as_str(),
                    "productName": skipped.product_name,
                    "sku": skipped.sku,
                    "locationCode": skipped.location_code,
                    "buyQtyMilli": skipped.buy_qty_milli,
                    "reason": skipped.reason,
                }))
                .collect::<Vec<_>>(),
            // The batch's own figures, stated rather than left to a client
            // adding up the list it was just given.
            "shortages": shortages.len(),
            "ordered": plan.groups.iter().map(|group| group.lines.len()).sum::<usize>(),
        }
    })))
}

/// `stock_answer` — where one product stands right now.
///
/// Every figure is read through the same store functions the `/inventory`
/// screens use: the shelf from the stock read, the pipeline from the fold the
/// shortage report itself uses, the minimums from the tenant's own rules. No
/// total is computed here that a screen computes differently elsewhere, and
/// nothing at all is written.
///
/// # Errors
/// `422` when the product cannot be resolved to exactly one catalog item;
/// `500` on a store failure.
pub async fn execute_stock_answer(account: &Account, args: &Value) -> Result<Json<Value>, Problem> {
    let product = resolve_product(account, args).await?;
    // A service has no shelf — the ledger refuses to move one — so the reads
    // that only make sense for goods are skipped rather than answered with
    // zeroes that would read as "none left".
    let (levels, pipeline, rules) = if product.stocked {
        let levels = account
            .acc
            .inv_stock(&StockFilter {
                product_id: Some(product.id.clone()),
                location_id: None,
                include_virtual: false,
                include_zero: false,
            })
            .await
            .map_err(map_store_err)?;
        let pipeline = account
            .acc
            .inv_product_pipeline(&product.id)
            .await
            .map_err(map_store_err)?;
        let rules = account
            .acc
            .inv_reorder_rules(&ReorderRuleFilter {
                product_id: Some(product.id.clone()),
                location_id: None,
                include_inactive: false,
            })
            .await
            .map_err(map_store_err)?;
        (levels, pipeline, rules)
    } else {
        (Vec::new(), ProductPipeline::default(), Vec::new())
    };
    let on_hand: i64 = levels.iter().map(|level| level.qty_milli).sum();

    Ok(Json(json!({
        "ok": true,
        "result": {
            "kind": "stockAnswer",
            "id": product.id.as_str(),
            "title": product.name,
            "product": product_json(&product),
            "stock": levels.iter().map(level_json).collect::<Vec<_>>(),
            "onHandQtyMilli": on_hand,
            "onOrderQtyMilli": pipeline.on_order_qty_milli,
            "committedQtyMilli": pipeline.committed_qty_milli,
            // The one number a reader would otherwise work out themselves, and
            // the one the shortage rule is actually tested against.
            "availableQtyMilli": available_qty_milli(
                on_hand,
                pipeline.on_order_qty_milli,
                pipeline.committed_qty_milli,
            ),
            "valueCents": levels.iter().map(|level| level.value_cents).sum::<i64>(),
            "watched": rules.iter().map(|rule| watch_json(rule, &levels)).collect::<Vec<_>>(),
        }
    })))
}

/// One place this product is watched, and whether the shelf there is under the
/// minimum somebody set for it.
///
/// The comparison is against **that shelf**, not against the tenant-wide
/// available quantity the shortage report uses, and the difference is
/// deliberate: this answer is read by somebody standing in a warehouse asking
/// what is on this shelf. The report's own verdict — the one that decides
/// whether to buy — stays in `reorder_proposals`, where the pipeline is part of
/// the sum.
fn watch_json(rule: &ReorderRule, levels: &[StockLevel]) -> Value {
    let here = levels
        .iter()
        .find(|level| level.location_id == rule.location_id)
        .map_or(0, |level| level.qty_milli);
    json!({
        "locationId": rule.location_id.as_str(),
        "locationCode": rule.location_code,
        "locationName": rule.location_name,
        "minQtyMilli": rule.min_qty_milli,
        "targetQtyMilli": rule.target_qty_milli,
        "onHandQtyMilli": here,
        "belowMinimum": here < rule.min_qty_milli,
    })
}

/// One draft to write: the supplier, the currency their offer is quoted in, and
/// the lines that go on it.
struct DraftGroup {
    supplier_id: InvSupplierId,
    currency: String,
    lines: Vec<NewPoLine>,
}

/// One shortage no draft was written for, and why.
struct SkippedShortage {
    product_id: BillingProductId,
    product_name: String,
    sku: String,
    location_code: String,
    buy_qty_milli: i64,
    reason: &'static str,
}

/// The whole batch, decided before anything is written.
struct OrderPlan {
    groups: Vec<DraftGroup>,
    skipped: Vec<SkippedShortage>,
}

/// Turns the shortage rows into the drafts to write, in one pure pass.
///
/// Pure, and separated from the writing for the reason every rule in this file
/// exists: what ends up on a purchase order is testable here without a
/// database, and the writing half then has no decision left to make.
///
/// Grouping is by supplier **and** currency (the module header argues it), and
/// the groups keep the order the shortage query returned — which is by product
/// name — so two runs over the same shelves produce the same documents.
fn plan_orders(shortages: &[Shortage], products: &HashMap<&str, &Product>) -> OrderPlan {
    let mut groups: Vec<DraftGroup> = Vec::new();
    let mut skipped = Vec::new();
    for shortage in shortages {
        let leave_out = |reason| SkippedShortage {
            product_id: shortage.product_id.clone(),
            product_name: shortage.product_name.clone(),
            sku: shortage.sku.clone(),
            location_code: shortage.location_code.clone(),
            buy_qty_milli: shortage.buy_qty_milli,
            reason,
        };
        // No offer is no order: buying from a supplier nobody quoted, at a price
        // nobody agreed, is exactly the invention this tool set forbids.
        let Some(offer) = shortage.supplier.as_ref() else {
            skipped.push(leave_out("noSupplier"));
            continue;
        };
        // Defensive, and honest if it ever fires: a row that asks for nothing is
        // a line that would be refused by the store's own quantity rule.
        if shortage.buy_qty_milli <= 0 {
            skipped.push(leave_out("nothingToBuy"));
            continue;
        }
        let line = NewPoLine {
            product_id: Some(shortage.product_id.clone()),
            line: NewLine {
                description: shortage.product_name.clone(),
                unit: shortage.unit.clone(),
                qty_milli: shortage.buy_qty_milli,
                unit_price_cents: offer.purchase_price_cents,
                // The rate the catalog carries. A product that vanished between
                // the two reads is ordered at no rate rather than at a guessed
                // one — the line is still a line, and a wrong tax rate on a
                // purchase order is a wrong bill three weeks later.
                vat_rate_bp: products
                    .get(shortage.product_id.as_str())
                    .map_or(0, |product| product.vat_rate_bp),
            },
        };
        match groups.iter_mut().find(|group| {
            group.supplier_id == offer.supplier_id && group.currency == offer.currency
        }) {
            Some(group) => group.lines.push(line),
            None => groups.push(DraftGroup {
                supplier_id: offer.supplier_id.clone(),
                currency: offer.currency.clone(),
                lines: vec![line],
            }),
        }
    }
    OrderPlan { groups, skipped }
}

/// Writes one group as a draft order: the header, then its lines.
///
/// The currency is stated rather than left to the supplier's default, because
/// the lines are priced in the **offer's** currency and a document whose header
/// says one thing and whose lines mean another is a total nobody can trust.
async fn write_draft(account: &Account, group: &DraftGroup) -> Result<InvPurchaseOrderId, Problem> {
    let id = account
        .acc
        .create_inv_purchase_order(&NewPurchaseOrder {
            currency: Some(group.currency.clone()),
            ..NewPurchaseOrder::for_supplier(group.supplier_id.clone())
        })
        .await
        .map_err(map_store_err)?;
    account
        .acc
        .set_inv_purchase_order_lines(&id, &group.lines)
        .await
        .map_err(map_store_err)?;
    Ok(id)
}

/// Reads a written draft back, so every figure in the answer is the server's own
/// — the totals included, which the browser never computes.
async fn draft_json(
    account: &Account,
    id: &InvPurchaseOrderId,
    lines: &[NewPoLine],
) -> Result<Value, Problem> {
    let today = OffsetDateTime::now_utc().date();
    let document = account
        .acc
        .inv_purchase_order(id)
        .await
        .map_err(map_store_err)?
        // Unreachable: it was written a statement ago through this same handle.
        .ok_or_else(|| unprocessable("the draft order could not be read back"))?;
    let mut json = document_json(&document, today);
    if let Some(object) = json.as_object_mut() {
        object.insert("lineCount".to_owned(), json!(lines.len()));
    }
    Ok(json)
}

/// The supplier a proposal names, resolved among the tenant's active suppliers.
///
/// `None` when the proposal named none, which is "every supplier" — the whole
/// Monday-morning question.
async fn resolve_supplier(
    account: &Account,
    args: &Value,
) -> Result<Option<(InvSupplierId, String)>, Problem> {
    let Some(wanted) = string_arg(args, "supplier") else {
        return Ok(None);
    };
    let suppliers = account
        .acc
        .inv_suppliers(false)
        .await
        .map_err(map_store_err)?;
    let picked = pick(
        &wanted,
        suppliers
            .iter()
            .map(|supplier| (supplier.name.as_str(), supplier))
            .collect(),
        "supplier",
    )?;
    Ok(Some((picked.id.clone(), picked.name.clone())))
}

/// The place a proposal names, resolved among the tenant's real stock
/// locations.
///
/// A code is tried first, exactly and case-insensitively, because a code is
/// what a warehouse says out loud ("VAN1") and it is short enough to appear
/// inside somebody's location *name*. Failing that the shared name rule applies,
/// with its refusal that lists the candidates rather than guessing.
///
/// Only real shelves are candidates: the virtual counterparties are an
/// accounting fact about a closed ledger, and nothing is ever short at one.
async fn resolve_location(account: &Account, args: &Value) -> Result<Option<Location>, Problem> {
    let Some(wanted) = string_arg(args, "location") else {
        return Ok(None);
    };
    let places: Vec<Location> = account
        .acc
        .inv_locations(false)
        .await
        .map_err(map_store_err)?
        .into_iter()
        .filter(|place| place.kind == LocationKind::Stock)
        .collect();
    if let Some(by_code) = places
        .iter()
        .find(|place| place.code.eq_ignore_ascii_case(wanted.trim()))
    {
        return Ok(Some(by_code.clone()));
    }
    let picked = pick(
        &wanted,
        places
            .iter()
            .map(|place| (place.name.as_str(), place))
            .collect(),
        "location",
    )?;
    Ok(Some(picked.clone()))
}

/// The product a question is about, resolved among the tenant's active catalog.
///
/// A barcode and an SKU are tried exactly first — both are machine-readable
/// identifiers a person may well have read off a box, and neither is a name to
/// match loosely — and only then does the shared name rule apply.
async fn resolve_product(account: &Account, args: &Value) -> Result<Product, Problem> {
    let wanted = string_arg(args, "product")
        .or_else(|| string_arg(args, "productName"))
        .ok_or_else(|| unprocessable("which product this is about is required"))?;
    let products = account
        .acc
        .billing_products(false)
        .await
        .map_err(map_store_err)?;
    if let Some(exact) = products.iter().find(|product| {
        (!product.sku.is_empty() && product.sku.eq_ignore_ascii_case(wanted.trim()))
            || (!product.barcode.is_empty() && product.barcode == wanted.trim())
    }) {
        return Ok(exact.clone());
    }
    let picked = pick(
        &wanted,
        products
            .iter()
            .map(|product| (product.name.as_str(), product))
            .collect(),
        "product",
    )?;
    Ok(picked.clone())
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    use alo_store::inv_reorder::ShortageSupplier;
    use alo_store::{InvLocationId, InvReorderRuleId};

    fn product(id: &str, name: &str, vat_rate_bp: i32) -> Product {
        Product {
            id: BillingProductId::new(id.to_owned()),
            name: name.to_owned(),
            unit: "piece".to_owned(),
            unit_price_cents: 8_600,
            vat_rate_bp,
            sku: format!("SKU-{id}"),
            barcode: String::new(),
            stocked: true,
            purchase_price_cents: 4_300,
            photo_node_id: None,
            default_supplier_id: None,
            archived_at: None,
            created_by: "u1".to_owned(),
            created_at: OffsetDateTime::now_utc(),
            updated_at: OffsetDateTime::now_utc(),
        }
    }

    fn offer(supplier: &str, currency: &str, price_cents: i64) -> ShortageSupplier {
        ShortageSupplier {
            supplier_id: InvSupplierId::new(supplier.to_owned()),
            supplier_name: format!("Supplier {supplier}"),
            supplier_code: String::new(),
            purchase_price_cents: price_cents,
            currency: currency.to_owned(),
            min_order_qty_milli: 0,
            lead_time_days: 9,
        }
    }

    fn shortage(product: &str, buy_qty_milli: i64, supplier: Option<ShortageSupplier>) -> Shortage {
        Shortage {
            rule_id: InvReorderRuleId::new(format!("r-{product}")),
            product_id: BillingProductId::new(product.to_owned()),
            product_name: format!("Item {product}"),
            sku: format!("SKU-{product}"),
            unit: "piece".to_owned(),
            location_id: InvLocationId::new("main".to_owned()),
            location_code: "MAIN".to_owned(),
            location_name: "Main warehouse".to_owned(),
            min_qty_milli: 4_000,
            target_qty_milli: 20_000,
            on_hand_qty_milli: 1_000,
            on_order_qty_milli: 0,
            committed_qty_milli: 0,
            available_qty_milli: 1_000,
            short_by_qty_milli: 3_000,
            buy_qty_milli,
            supplier,
            estimated_cost_cents: 0,
        }
    }

    fn catalog(products: &[Product]) -> HashMap<&str, &Product> {
        products
            .iter()
            .map(|product| (product.id.as_str(), product))
            .collect()
    }

    #[test]
    fn one_supplier_gets_one_draft_carrying_every_line_they_quote_for() {
        let products = [
            product("p1", "Item p1", 1900),
            product("p2", "Item p2", 900),
        ];
        let plan = plan_orders(
            &[
                shortage("p1", 19_000, Some(offer("s1", "EUR", 3_150))),
                shortage("p2", 5_000, Some(offer("s1", "EUR", 1_200))),
            ],
            &catalog(&products),
        );
        assert_eq!(plan.groups.len(), 1, "one supplier is one document");
        let group = &plan.groups[0];
        assert_eq!(group.supplier_id.as_str(), "s1");
        assert_eq!(group.currency, "EUR");
        assert_eq!(group.lines.len(), 2);
        // Every number on the line came from the store: the quantity from the
        // shortage arithmetic, the price from the offer, the rate from the
        // catalog. None of them is composed here.
        assert_eq!(group.lines[0].line.qty_milli, 19_000);
        assert_eq!(group.lines[0].line.unit_price_cents, 3_150);
        assert_eq!(group.lines[0].line.vat_rate_bp, 1900);
        assert_eq!(group.lines[1].line.vat_rate_bp, 900);
        assert!(plan.skipped.is_empty());
    }

    #[test]
    fn two_suppliers_are_two_drafts_and_two_currencies_are_two_more() {
        let products = [
            product("p1", "Item p1", 1900),
            product("p2", "Item p2", 1900),
            product("p3", "Item p3", 1900),
        ];
        let plan = plan_orders(
            &[
                shortage("p1", 1_000, Some(offer("s1", "EUR", 100))),
                shortage("p2", 1_000, Some(offer("s2", "EUR", 100))),
                // The same supplier, quoting in another currency: a second
                // document, because one order whose lines are in two currencies
                // has a total in neither.
                shortage("p3", 1_000, Some(offer("s1", "USD", 100))),
            ],
            &catalog(&products),
        );
        assert_eq!(plan.groups.len(), 3);
        assert_eq!(plan.groups[0].currency, "EUR");
        assert_eq!(plan.groups[2].supplier_id.as_str(), "s1");
        assert_eq!(plan.groups[2].currency, "USD");
        for group in &plan.groups {
            assert_eq!(group.lines.len(), 1);
        }
    }

    #[test]
    fn a_shortage_nobody_quotes_for_is_left_out_and_says_so() {
        let products = [product("p1", "Item p1", 1900)];
        let plan = plan_orders(&[shortage("p1", 9_000, None)], &catalog(&products));
        assert!(plan.groups.is_empty(), "nothing may be ordered blind");
        assert_eq!(plan.skipped.len(), 1);
        assert_eq!(plan.skipped[0].reason, "noSupplier");
        // The left-out line still names what it was about, and how much was
        // needed — the reader has to be able to act on it by hand.
        assert_eq!(plan.skipped[0].product_id.as_str(), "p1");
        assert_eq!(plan.skipped[0].buy_qty_milli, 9_000);
        assert_eq!(plan.skipped[0].location_code, "MAIN");
    }

    #[test]
    fn a_row_that_asks_for_nothing_never_becomes_a_line() {
        let products = [product("p1", "Item p1", 1900)];
        let plan = plan_orders(
            &[shortage("p1", 0, Some(offer("s1", "EUR", 100)))],
            &catalog(&products),
        );
        assert!(plan.groups.is_empty());
        assert_eq!(plan.skipped[0].reason, "nothingToBuy");
    }

    #[test]
    fn a_product_that_left_the_catalog_is_ordered_at_no_rate_rather_than_a_guessed_one() {
        let plan = plan_orders(
            &[shortage("gone", 1_000, Some(offer("s1", "EUR", 100)))],
            &catalog(&[]),
        );
        assert_eq!(plan.groups[0].lines[0].line.vat_rate_bp, 0);
    }

    #[test]
    fn nothing_at_all_is_a_plan_with_nothing_in_it() {
        let plan = plan_orders(&[], &catalog(&[]));
        assert!(plan.groups.is_empty());
        assert!(plan.skipped.is_empty());
    }

    #[test]
    fn a_watched_shelf_is_judged_by_what_is_on_that_shelf() {
        let rule = ReorderRule {
            id: InvReorderRuleId::new("r1".to_owned()),
            product_id: BillingProductId::new("p1".to_owned()),
            product_name: "Item p1".to_owned(),
            sku: "SKU-p1".to_owned(),
            unit: "piece".to_owned(),
            location_id: InvLocationId::new("main".to_owned()),
            location_code: "MAIN".to_owned(),
            location_name: "Main warehouse".to_owned(),
            min_qty_milli: 4_000,
            target_qty_milli: 20_000,
            active: true,
            created_by: "u1".to_owned(),
            created_at: OffsetDateTime::now_utc(),
            updated_at: OffsetDateTime::now_utc(),
        };
        let level = |location: &str, qty_milli: i64| StockLevel {
            product_id: BillingProductId::new("p1".to_owned()),
            product_name: "Item p1".to_owned(),
            sku: "SKU-p1".to_owned(),
            location_id: InvLocationId::new(location.to_owned()),
            location_code: location.to_uppercase(),
            location_name: location.to_owned(),
            location_kind: LocationKind::Stock,
            qty_milli,
            value_cents: 0,
            last_move_at: OffsetDateTime::now_utc(),
        };

        // Plenty in the van does not make the watched shelf full.
        let elsewhere = watch_json(&rule, &[level("van1", 99_000)]);
        assert_eq!(elsewhere["onHandQtyMilli"], 0);
        assert_eq!(elsewhere["belowMinimum"], true);

        let here = watch_json(&rule, &[level("main", 4_000), level("van1", 1_000)]);
        assert_eq!(here["onHandQtyMilli"], 4_000);
        // Exactly the minimum is not under it.
        assert_eq!(here["belowMinimum"], false);
        assert_eq!(here["locationCode"], "MAIN");
        assert_eq!(here["targetQtyMilli"], 20_000);
    }

    #[test]
    fn a_quantity_or_a_price_the_model_states_reaches_nothing() {
        // The rule the module header states, as code: the executor's arguments
        // are two narrowings, so a quantity or a price smuggled into them is
        // read by nothing. `plan_orders` never sees the args at all — the only
        // way a number could reach a line is through the store's own rows.
        let products = [product("p1", "Item p1", 1900)];
        let planned = plan_orders(
            &[shortage("p1", 19_000, Some(offer("s1", "EUR", 3_150)))],
            &catalog(&products),
        );
        assert_eq!(planned.groups[0].lines[0].line.qty_milli, 19_000);
        assert_eq!(planned.groups[0].lines[0].line.unit_price_cents, 3_150);
    }
}
