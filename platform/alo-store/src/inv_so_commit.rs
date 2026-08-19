//! What a confirmation may promise (alo Orders, ADR 0054 §3) — the refusal that
//! stops the same fan being sold twice.
//!
//! [`crate::inv_so_confirm`] draws the number, stamps the day and freezes the
//! document. This module answers the one question it was never asking: **can the
//! goods exist?** It is a separate file because the two have different reasons to
//! change — one is about a document's lifecycle, the other about a warehouse.
//!
//! ## There is no `reserved` column, and that is the decision
//!
//! The quantity is already computed. `inv_reorder`'s [`COMMITTED_SQL`] has folded
//! *the undelivered remainder of every confirmed sales-order line* since wave
//! B5.07, and says in its own header why it is not a table: a stored figure is a
//! second thing that must be kept in step with the orders. So this module stores
//! nothing and adds no schema; it re-uses that fold rather than restating it,
//! because a second reading of "promised out" that disagreed with the buyer's
//! shortage report by one line would be two truths about one shelf.
//!
//! The lifecycle comes free with the fold. Confirming promises the remainder;
//! delivering releases it, because what has left is no longer outstanding;
//! cancelling releases it, because a cancelled order is not one of the two
//! reserving states. There is no hook anywhere to forget.
//!
//! ## Why the order's own row lock is not enough
//!
//! `confirm_inv_sales_order` already holds `SELECT … FOR UPDATE` on the order.
//! That serialises two confirmations *of the same order* and nothing else — two
//! different orders for the same product lock different rows, and their counts
//! interleave freely between the read and the write. A refusal built on that lock
//! alone passes every single-threaded test and fails the first time two people
//! confirm in the same second, which is the entire failure this wave is named
//! after.
//!
//! So the count and the decision are made one act by a **transaction-scoped
//! advisory lock per `(tenant, product)`** — the same instrument
//! [`crate::inv_stock_sale`] already uses for the same race, so the two paths in
//! this product that can promise a unit settle contention one way instead of two.
//!
//! **Every stocked product on the order is locked, in ascending product-id
//! order.** An order has many lines where a shop hold has one, and two orders
//! sharing two products, each locking them in the order they happen to appear,
//! deadlock. Sorting makes that unrepresentable rather than unlikely.
//!
//! ## What it deliberately does not consult
//!
//! Live shop holds ([`crate::inv_stock_sale`]) are **not** subtracted, and that
//! module's own decision — that Inventory's doors do not consult a shop table —
//! is left standing (ADR 0054 §2). The consequence is named rather than hidden: a
//! hold and a confirmed order can both count on the same unit, and whoever picks
//! second finds it gone. `record_move` refuses to take the ledger negative, so
//! that surfaces as honest scarcity at pick time and never as an oversold shelf.

use crate::error::{Result, StoreError};
use crate::inv_reorder::{COMMITTED_SQL, ON_ORDER_SQL, available_qty_milli};

/// On-hand per product across the tenant's **real stock** locations.
///
/// Tenant-wide because an order line names no shelf: a promise is made by the
/// business, not by a warehouse. This is [`crate::inv_stock_sale`]'s reading of
/// on-hand (`l.kind = 'stock'`) rather than a third one — virtual counterparty
/// locations are not goods we have.
const ON_HAND_SQL: &str = "SELECT s.product_id, SUM(s.qty_milli)::bigint AS qty_milli \
     FROM inv_stock s \
     JOIN inv_locations l ON l.tenant_id = s.tenant_id AND l.id = s.location_id \
     WHERE s.tenant_id = $1 AND l.kind = 'stock' \
     GROUP BY s.product_id";

/// What one product on the order needs, and what stands behind it.
///
/// The product is carried by **name** rather than by id: the only consumer is a
/// sentence somebody reads. Which product is reported first when several are
/// short is settled by the query's `ORDER BY`, so the refusal is the same
/// sentence every time rather than whichever row the planner returned first.
#[derive(Debug, Clone, sqlx::FromRow)]
struct Standing {
    name: String,
    wanted_qty_milli: i64,
    on_hand_qty_milli: i64,
    on_order_qty_milli: i64,
    committed_qty_milli: i64,
}

/// Refuses `order` if confirming it would promise goods that cannot exist.
///
/// Runs **inside the confirming transaction**, after its row lock and before
/// anything is written, so a refusal leaves the order exactly as it was: still a
/// draft, still unnumbered, still deletable.
///
/// Lines naming no product, or naming one that is not stocked, are skipped
/// entirely — a service has no shelf to draw on, and an order of consultancy
/// days must never be blocked by an empty warehouse.
///
/// # Errors
/// [`StoreError::Conflict`] naming the product and how many are short — a
/// salesperson has to know what to tell the customer — and [`StoreError::Db`] on
/// failure.
pub(crate) async fn refuse_over_commitment(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    tenant: &str,
    order_id: &str,
) -> Result<()> {
    // What this order asks of the shelf, one row per product. Summed, because
    // two lines of one order may name the same product and checking them
    // separately would let an order over-promise against itself.
    let wanted: Vec<(String, i64)> = sqlx::query_as(
        "SELECT sol.product_id, SUM(GREATEST(sol.qty_milli, 0))::bigint \
         FROM inv_sales_order_lines sol \
         JOIN billing_products p ON p.tenant_id = sol.tenant_id AND p.id = sol.product_id \
         WHERE sol.tenant_id = $1 AND sol.so_id = $2 AND sol.product_id IS NOT NULL \
           AND p.stocked \
         GROUP BY sol.product_id \
         ORDER BY sol.product_id",
    )
    .bind(tenant)
    .bind(order_id)
    .fetch_all(&mut **tx)
    .await
    .map_err(StoreError::Db)?;
    if wanted.is_empty() {
        return Ok(());
    }

    // One writer at a time per product, for the rest of the transaction. Taken
    // in the order the query returned them, which is ascending product id: two
    // orders sharing two products must ask for them in the same sequence or they
    // deadlock.
    for (product_id, _) in &wanted {
        sqlx::query("SELECT pg_advisory_xact_lock(hashtext($1), hashtext($2))")
            .bind(tenant)
            .bind(product_id)
            .execute(&mut **tx)
            .await
            .map_err(StoreError::Db)?;
    }

    // Now count, under the locks. The two pipeline folds are inventory's own,
    // spliced in rather than restated.
    let ids: Vec<String> = wanted.iter().map(|(id, _)| id.clone()).collect();
    let quantities: Vec<i64> = wanted.iter().map(|(_, qty)| *qty).collect();
    let standing = sqlx::query_as::<_, Standing>(&format!(
        "SELECT p.name, w.wanted_qty_milli, \
             COALESCE(oh.qty_milli, 0) AS on_hand_qty_milli, \
             COALESCE(oo.qty_milli, 0) AS on_order_qty_milli, \
             COALESCE(cm.qty_milli, 0) AS committed_qty_milli \
         FROM (SELECT * FROM unnest($2::text[], $3::bigint[]) \
                   AS t(product_id, wanted_qty_milli)) w \
         JOIN billing_products p ON p.tenant_id = $1 AND p.id = w.product_id \
         LEFT JOIN ({ON_HAND_SQL}) oh ON oh.product_id = w.product_id \
         LEFT JOIN ({ON_ORDER_SQL}) oo ON oo.product_id = w.product_id \
         LEFT JOIN ({COMMITTED_SQL}) cm ON cm.product_id = w.product_id \
         ORDER BY w.product_id"
    ))
    .bind(tenant)
    .bind(&ids)
    .bind(&quantities)
    .fetch_all(&mut **tx)
    .await
    .map_err(StoreError::Db)?;

    for row in &standing {
        let available = available_qty_milli(
            row.on_hand_qty_milli,
            row.on_order_qty_milli,
            row.committed_qty_milli,
        );
        if row.wanted_qty_milli > available {
            return Err(StoreError::Conflict(shortfall_message(row, available)));
        }
    }
    Ok(())
}

/// What the salesperson reads. Quantities are shown in whole units where they
/// divide evenly, because "2 short" is what somebody says on the telephone and
/// "2000 short" is what a database says.
fn shortfall_message(row: &Standing, available: i64) -> String {
    let short = row.wanted_qty_milli.saturating_sub(available);
    format!(
        "{} is short by {}: the order asks for {} and {} can be promised \
         (on hand {}, on order {}, already promised {})",
        row.name,
        units(short),
        units(row.wanted_qty_milli),
        units(available.max(0)),
        units(row.on_hand_qty_milli),
        units(row.on_order_qty_milli),
        units(row.committed_qty_milli),
    )
}

/// Milli-units as a person would say them: whole where they are whole, three
/// decimals where they are not.
fn units(qty_milli: i64) -> String {
    if qty_milli % 1_000 == 0 {
        return (qty_milli / 1_000).to_string();
    }
    format!("{}.{:03}", qty_milli / 1_000, (qty_milli % 1_000).abs())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn standing(wanted: i64, on_hand: i64, on_order: i64, committed: i64) -> Standing {
        Standing {
            name: "AF-630 axial fan".to_owned(),
            wanted_qty_milli: wanted,
            on_hand_qty_milli: on_hand,
            on_order_qty_milli: on_order,
            committed_qty_milli: committed,
        }
    }

    #[test]
    fn a_quantity_reads_the_way_somebody_would_say_it() {
        assert_eq!(units(2_000), "2");
        assert_eq!(units(0), "0");
        assert_eq!(units(1), "0.001");
        assert_eq!(units(2_500), "2.500");
        assert_eq!(units(-1_000), "-1");
    }

    #[test]
    fn the_refusal_says_the_product_the_shortfall_and_where_the_number_came_from() {
        // Six wanted, four on the shelf, none on order, none promised.
        let row = standing(6_000, 4_000, 0, 0);
        let available = available_qty_milli(4_000, 0, 0);
        let said = shortfall_message(&row, available);
        assert!(said.contains("AF-630 axial fan"), "{said}");
        assert!(said.contains("short by 2"), "{said}");
        assert!(said.contains("asks for 6"), "{said}");
        assert!(said.contains("on hand 4"), "{said}");
        // The three parts are shown separately so a reader can see which one is
        // the problem — an order refused because of somebody else's order reads
        // very differently from one refused because the shelf is empty.
        assert!(
            said.contains("on order 0") && said.contains("already promised 0"),
            "{said}"
        );
    }

    #[test]
    fn a_promise_already_made_is_what_makes_the_next_one_short() {
        // The case the whole module exists for: goods are there, but spoken for.
        let row = standing(1_000, 1_000, 0, 1_000);
        let available = available_qty_milli(1_000, 0, 1_000);
        assert_eq!(available, 0);
        let said = shortfall_message(&row, available);
        assert!(said.contains("already promised 1"), "{said}");
        assert!(said.contains("short by 1"), "{said}");
    }

    #[test]
    fn goods_on_their_way_in_count_toward_what_may_be_promised() {
        // Nothing on the shelf and six on order: a business that could not
        // promise what it has already bought could not take an order at all.
        assert_eq!(available_qty_milli(0, 6_000, 0), 6_000);
        // And the seventh is still short, because what is on order is finite.
        let row = standing(7_000, 0, 6_000, 0);
        let said = shortfall_message(&row, 6_000);
        assert!(said.contains("short by 1"), "{said}");
    }

    #[test]
    fn availability_never_reads_as_a_negative_promise() {
        // An over-delivered or over-committed history must not print
        // "-3 can be promised" at a salesperson.
        let row = standing(1_000, 0, 0, 3_000);
        let said = shortfall_message(&row, available_qty_milli(0, 0, 3_000));
        assert!(said.contains("0 can be promised"), "{said}");
    }
}
