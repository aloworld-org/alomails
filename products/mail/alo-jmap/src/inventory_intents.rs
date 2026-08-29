//! The executors of alo Inventory's verbs (ADR 0058) — what runs when the
//! Inventory agent uses one of the intents `alo_ai::inventory_intents`
//! describes.
//!
//! Every executor runs through the asker's account door
//! ([`crate::state::Account::acc`], the tenant-scoped store), and answers with
//! the same record views the `/inventory/*` routes serve — the shortage report
//! through [`crate::inventory_reorder::shortage_json`], the order list through
//! [`crate::inventory_po::summary_json`], the price list through
//! [`crate::inventory_supplier_prices::price_json`], the ledger through
//! [`crate::inventory_moves::move_json`] — with money made readable beside its
//! integers ([`crate::billing_intents::ok`], the shared rendering). A write
//! only ever runs from the asker's approval ([`crate::agent::execute_tool`]
//! holds that).
//!
//! **Resolution is the executor's job, not the model's.** A supplier, a place
//! and a product are the user's words resolved against the tenant's own
//! records ([`crate::agent_inventory`]'s resolvers, shared with the kept
//! executors); an order is found by its number exactly, or by its supplier's
//! name when only one of theirs is still open. Nothing here guesses, and an
//! ambiguity is a refusal that lists the candidates.
//!
//! The kept executors stay in their own file and are reached only from the
//! dispatch below: [`crate::agent_inventory`] (the draft reorders and the
//! stock answer).

use serde_json::{Value, json};

use alo_store::InvPurchaseOrderId;
use alo_store::inv_moves::MoveFilter;
use alo_store::inv_po::PoStatus;
use alo_store::inv_po_receive::NewReceipt;
use alo_store::inv_reorder::ShortageFilter;

use crate::agent_args::{pick, string_arg, unprocessable};
use crate::agent_inventory::{resolve_location, resolve_product, resolve_supplier};
use crate::billing::map_store_err;
use crate::billing_document::today;
use crate::billing_intents::{Reply, ok};
use crate::error::Problem;
use crate::state::{Account, AppState};

/// How many records a list read returns — enough for a question, small enough
/// to sit inside the turn's result window.
const MAX_LISTED: usize = 12;

/// `stock_below_minimum` — the shortage report exactly as `GET
/// /inventory/shortages` serves it, narrowed to one supplier or one place when
/// the asker named one. A shortage nobody quotes for keeps its `null` supplier:
/// assigning one is the invention this module forbids.
pub async fn execute_stock_below_minimum(account: &Account, args: &Value) -> Reply {
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
    let listed: Vec<Value> = shortages
        .iter()
        .take(MAX_LISTED)
        .map(crate::inventory_reorder::shortage_json)
        .collect();
    ok(json!({
        "kind": "stockBelowMinimum",
        // What the caller narrowed to, echoed so the answer can say what it
        // looked at rather than implying it looked at everything.
        "supplier": supplier.map(|(_, name)| name),
        "location": location.map(|place| place.name),
        "shortageCount": shortages.len(),
        "shown": listed.len(),
        "shortages": listed,
    }))
}

/// The status filter the asker named, or `None` for the default — everything
/// unfinished. A word that is not a status is a refusal that lists them.
fn wanted_status(args: &Value) -> Result<Option<PoStatus>, Problem> {
    match string_arg(args, "status")
        .map(|word| word.trim().to_ascii_lowercase())
        .filter(|word| !word.is_empty())
    {
        None => Ok(None),
        Some(word) => PoStatus::parse(&word).map(Some).ok_or_else(|| {
            unprocessable(format!(
                "no purchase-order status is called \"{word}\" — say one of draft, sent, partially_received, received, cancelled"
            ))
        }),
    }
}

/// Whether an order is in scope: the named status exactly, or — by default —
/// not finished with, which is what "open" means when somebody asks what is
/// on order.
fn in_scope(wanted: Option<PoStatus>, status: PoStatus) -> bool {
    match wanted {
        Some(one) => status == one,
        None => !status.is_closed(),
    }
}

/// `open_purchase_orders` — the order list exactly as `GET
/// /inventory/purchase-orders` serves it: each order with its supplier, its
/// computed totals and its `late` flag, filtered to what is unfinished unless
/// a status is named, and to one supplier when the asker named one.
pub async fn execute_open_purchase_orders(account: &Account, args: &Value) -> Reply {
    let wanted = wanted_status(args)?;
    let supplier = resolve_supplier(account, args).await?;
    let orders = account
        .acc
        .inv_purchase_orders(wanted)
        .await
        .map_err(map_store_err)?;
    let in_scope: Vec<_> = orders
        .iter()
        .filter(|entry| in_scope(wanted, entry.order.status))
        .filter(|entry| {
            supplier
                .as_ref()
                .is_none_or(|(id, _)| entry.order.supplier_id == *id)
        })
        .collect();
    let day = today();
    let listed: Vec<Value> = in_scope
        .iter()
        .take(MAX_LISTED)
        .map(|entry| crate::inventory_po::summary_json(entry, day))
        .collect();
    ok(json!({
        "kind": "purchaseOrders",
        "status": wanted.map(PoStatus::as_str),
        "supplier": supplier.map(|(_, name)| name),
        "orderCount": in_scope.len(),
        "lateCount": in_scope.iter().filter(|entry| entry.order.is_late(day)).count(),
        "shown": listed.len(),
        "purchaseOrders": listed,
    }))
}

/// `supplier_prices` — one supplier's price list exactly as `GET
/// /inventory/suppliers/{id}/products` serves it, the supplier resolved from
/// the asker's word among the tenant's active suppliers.
pub async fn execute_supplier_prices(account: &Account, args: &Value) -> Reply {
    let wanted = string_arg(args, "supplier")
        .ok_or_else(|| unprocessable("which supplier this is about is required"))?;
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
    let offers = account
        .acc
        .inv_supplier_prices(&picked.id)
        .await
        .map_err(map_store_err)?;
    let listed: Vec<Value> = offers
        .iter()
        .take(MAX_LISTED)
        .map(|offer| crate::inventory_supplier_prices::price_json(offer, picked.lead_time_days))
        .collect();
    ok(json!({
        "kind": "supplierPrices",
        "supplierId": picked.id.as_str(),
        "supplierName": picked.name,
        "offerCount": offers.len(),
        "shown": listed.len(),
        "offers": listed,
    }))
}

/// `recent_moves` — the stock ledger's newest movements exactly as `GET
/// /inventory/moves` serves them, narrowed to one product or one place when
/// the asker named one. The cap is this module's listing bound, stated back
/// as `shown`.
pub async fn execute_recent_moves(account: &Account, args: &Value) -> Reply {
    let product = match string_arg(args, "product").filter(|name| !name.trim().is_empty()) {
        Some(_) => Some(resolve_product(account, args).await?),
        None => None,
    };
    let location = resolve_location(account, args).await?;
    let moves = account
        .acc
        .inv_moves(&MoveFilter {
            product_id: product.as_ref().map(|one| one.id.clone()),
            location_id: location.as_ref().map(|place| place.id.clone()),
            from: None,
            to: None,
            limit: Some(MAX_LISTED as i64),
        })
        .await
        .map_err(map_store_err)?;
    ok(json!({
        "kind": "recentMoves",
        "product": product.map(|one| one.name),
        "location": location.map(|place| place.name),
        "shown": moves.len(),
        "moves": moves
            .iter()
            .map(crate::inventory_moves::move_json)
            .collect::<Vec<_>>(),
    }))
}

/// The order a delivery names, resolved among the tenant's own orders: by its
/// number exactly first — a number is machine-readable and exact — then by the
/// supplier's name among the orders goods may still arrive against. Several
/// open orders of one supplier are a refusal that lists their numbers, never a
/// guess; a closed order named by number is passed through so the store's own
/// refusal can say *why* it cannot be received.
async fn resolve_order(account: &Account, wanted: &str) -> Result<InvPurchaseOrderId, Problem> {
    let orders = account
        .acc
        .inv_purchase_orders(None)
        .await
        .map_err(map_store_err)?;
    if let Some(by_number) = orders.iter().find(|entry| {
        entry
            .order
            .number
            .as_deref()
            .is_some_and(|number| number.eq_ignore_ascii_case(wanted.trim()))
    }) {
        return Ok(by_number.order.id.clone());
    }
    let needle = wanted.trim().to_lowercase();
    if needle.is_empty() {
        return Err(unprocessable("which order arrived is required"));
    }
    let open: Vec<_> = orders
        .iter()
        .filter(|entry| entry.order.status.is_open())
        .filter(|entry| entry.supplier_name.trim().to_lowercase().contains(&needle))
        .collect();
    match open.as_slice() {
        [] => Err(unprocessable(format!(
            "no open purchase order matches \"{wanted}\" — say the order's number"
        ))),
        [one] => Ok(one.order.id.clone()),
        several => Err(unprocessable(format!(
            "more than one open order matches \"{wanted}\": {} — say which number",
            several
                .iter()
                .map(|entry| entry.order.number.clone().unwrap_or_default())
                .collect::<Vec<_>>()
                .join(", ")
        ))),
    }
}

/// `receive_delivery` — book the arrival of everything still outstanding on
/// one placed order, exactly as `POST /inventory/purchase-orders/{id}/receipts`
/// does with no line set: the store moves the goods into the named place,
/// advances the order and raises the **draft** bill in one transaction. The
/// answer is the store's own read-back — the order as it now stands, the
/// receipt with the movements it wrote, and the bill's id.
pub async fn execute_receive_delivery(account: &Account, args: &Value) -> Reply {
    let wanted = string_arg(args, "order")
        .ok_or_else(|| unprocessable("which order arrived is required"))?;
    let place = resolve_location(account, args)
        .await?
        .ok_or_else(|| unprocessable("where the goods were put is required"))?;
    let id = resolve_order(account, &wanted).await?;
    let outcome = account
        .acc
        .receive_inv_purchase_order(
            &id,
            &NewReceipt {
                location_id: place.id.clone(),
                // No line set: everything still outstanding. A delivery that
                // differs from the order is booked line by line in the app.
                lines: None,
                note: string_arg(args, "note").unwrap_or_default(),
            },
        )
        .await
        .map_err(map_store_err)?;
    ok(json!({
        "kind": "delivery",
        "purchaseOrder": crate::inventory_po::document_json(&outcome.order, today()),
        "receipt": crate::inventory_po_receipts::receipt_json(&outcome.receipt),
        "billId": outcome.bill_id.as_str(),
    }))
}

/// The module's verbs by name (A4.1c) — Inventory's one row in the agent's
/// dispatcher list, `crate::agent::MODULES`. `None` is "not mine": the
/// dispatcher then asks the next module. The kept executors —
/// [`crate::agent_inventory`] for the draft reorders and the stock answer —
/// are reached from here so the agent has one place to look.
pub(crate) fn dispatch<'a>(
    _state: &'a AppState,
    account: &'a Account,
    tool: &'a str,
    args: &'a Value,
) -> Option<crate::agent::Dispatched<'a>> {
    let run: crate::agent::Dispatched<'a> = match tool {
        "stock_answer" => Box::pin(crate::agent_inventory::execute_stock_answer(account, args)),
        "stock_below_minimum" => Box::pin(execute_stock_below_minimum(account, args)),
        "open_purchase_orders" => Box::pin(execute_open_purchase_orders(account, args)),
        "supplier_prices" => Box::pin(execute_supplier_prices(account, args)),
        "recent_moves" => Box::pin(execute_recent_moves(account, args)),
        "reorder_proposals" => Box::pin(crate::agent_inventory::execute_reorder_proposals(
            account, args,
        )),
        "receive_delivery" => Box::pin(execute_receive_delivery(account, args)),
        _ => return None,
    };
    Some(run)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;
    use alo_ai::inventory_intents::INVENTORY;

    /// Every `/inventory` route the router registers is the adapter of a verb
    /// or excluded with a reason — the coverage ADR 0058 makes structural.
    #[test]
    fn every_inventory_route_is_a_verb_or_an_exclusion() {
        let router = include_str!("server.rs");
        let missing = INVENTORY.uncovered(router, "/inventory");
        assert!(
            missing.is_empty(),
            "routes with neither a verb nor a reason: {missing:?}"
        );
        // …and every verb's route exists, so an intent cannot name a route the
        // app does not have.
        let routes = alo_ai::routes_in(router, "/inventory");
        for intent in INVENTORY.intents {
            for route in intent.routes {
                assert!(
                    routes.contains(&(*route).to_owned()),
                    "{}: {route} is not a route",
                    intent.name
                );
            }
        }
    }

    #[test]
    fn every_verb_the_registry_offers_is_dispatched() {
        let dispatch = include_str!("inventory_intents.rs");
        for intent in INVENTORY.intents {
            assert!(
                dispatch.contains(&format!("\"{}\" =>", intent.name)),
                "{} has no executor in the dispatch",
                intent.name
            );
        }
    }

    /// Inventory's registration is one row in each list (A4.1c): the agent's
    /// dispatcher names this module once, the registry names it once, and the
    /// two lists are the same length — every moved module has its dispatcher.
    #[test]
    fn the_module_is_one_row_in_each_list() {
        let agent = include_str!("agent.rs");
        assert_eq!(
            agent.matches("inventory_intents::").count(),
            1,
            "agent.rs names Inventory only in MODULES"
        );
        assert!(agent.contains("crate::inventory_intents::dispatch"));
        assert_eq!(
            crate::agent::MODULES.len(),
            alo_ai::MOVED.len(),
            "a moved module without a dispatcher, or the reverse"
        );
    }

    #[test]
    fn the_default_scope_is_everything_unfinished_and_a_named_status_is_exact() {
        for still_going in [PoStatus::Draft, PoStatus::Sent, PoStatus::PartiallyReceived] {
            assert!(in_scope(None, still_going), "{still_going:?}");
        }
        for finished in [PoStatus::Received, PoStatus::Cancelled] {
            assert!(!in_scope(None, finished), "{finished:?}");
        }
        assert!(in_scope(Some(PoStatus::Received), PoStatus::Received));
        assert!(!in_scope(Some(PoStatus::Received), PoStatus::Sent));
        // The filter word is validated, and a stranger is a refusal that
        // lists the real ones rather than an empty list.
        assert_eq!(wanted_status(&json!({})).unwrap(), None);
        assert_eq!(
            wanted_status(&json!({ "status": " Sent " })).unwrap(),
            Some(PoStatus::Sent)
        );
        let refusal = wanted_status(&json!({ "status": "waiting" })).expect_err("no such status");
        let detail = refusal.detail.unwrap_or_default();
        assert!(detail.contains("no purchase-order status is called \"waiting\""));
        assert!(detail.contains("partially_received"), "{detail}");
    }
}
