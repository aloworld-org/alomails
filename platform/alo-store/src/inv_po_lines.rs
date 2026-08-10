//! The line of a purchase order (alo Inventory, ADR 0035, wave B5.05a) — what
//! we asked a supplier for, at the price they quoted when we asked.
//!
//! It is a [`crate::billing_line::Line`] **plus a product**, and that is the
//! whole of the difference. The description, unit, quantity, price and VAT rate
//! are the same five fields under the same rules, validated by the same
//! [`crate::billing_line`] code, because a line we buy and a line we sell are
//! the same shape of statement about a quantity of something at a price. What a
//! purchase-order line adds is the catalog link: the product this line will put
//! **into stock** when the goods arrive (B5.05b).
//!
//! The link is a link, not a snapshot, and the snapshot is everything else. A
//! line copies the description, unit, price and rate at the moment it is
//! drafted — re-negotiating with a supplier must never rewrite an order already
//! placed — while `product_id` stays a reference, because the movement receiving
//! writes has to name a product the stock ledger knows. Deleting the product
//! nulls the link and leaves the line's own words untouched, which is exactly
//! what the reader of a two-year-old order needs.
//!
//! **Two kinds of line, one table.** A line that names a product is goods: it
//! becomes a movement, so its quantity must be positive. A line that names none
//! is a charge in words — freight, packaging, a discount the supplier granted —
//! and may be negative, the same latitude a billing line has. That rule is
//! stated once, here, and tested directly.
//!
//! This module owns the two statements over `inv_purchase_order_lines` and
//! nothing else; the order's own record and its state machine are
//! [`crate::inv_po`]'s.

use crate::billing_line::{Line, LineRow, NewLine, NormalizedLine, normalize_lines};
use crate::billing_totals::LineFigures;
use crate::error::{Result, StoreError};
use crate::id::{BillingLineId, BillingProductId};

/// The writable shape of a purchase-order line: a billing line, plus the
/// product it orders when it orders one.
///
/// Lines are always written as a whole set, in the caller's order, so a line
/// carries no id and no position of its own on the way in.
#[derive(Debug, Clone, Default)]
pub struct NewPoLine {
    /// The catalog item this line orders, or `None` for a charge in words.
    /// Must be one of this tenant's products.
    pub product_id: Option<BillingProductId>,
    /// Description, unit, quantity, price and rate — the shared line model.
    pub line: NewLine,
}

/// A stored line of a purchase order.
#[derive(Debug, Clone)]
pub struct PoLine {
    /// The catalog item, or `None` for a charge in words — also `None` once a
    /// named product has been deleted from the catalog, which leaves the line
    /// itself readable exactly as it was agreed.
    pub product_id: Option<BillingProductId>,
    /// The shared line: id, print position, and the five snapshotted fields.
    pub line: Line,
    /// How much of this line has arrived, in the same milli-units it was
    /// ordered in (B5.05b). `0` on a line nothing has come in against, and on
    /// every charge in words — freight does not arrive on a pallet.
    ///
    /// An accumulator rather than a fold over the movement ledger, because two
    /// lines of one order may name the same product and the ledger could then
    /// not say which line a movement belongs to. Written only by the receiving
    /// transaction, which writes those movements in the same breath.
    pub received_qty_milli: i64,
}

impl PoLine {
    /// The three numbers this line contributes to the order's totals.
    pub fn figures(&self) -> LineFigures {
        self.line.figures()
    }

    /// Whether this line is goods that will move into stock when they arrive,
    /// rather than a charge in words.
    pub fn is_goods(&self) -> bool {
        self.product_id.is_some()
    }

    /// How much of this line is still to come, in milli-units.
    ///
    /// Zero for a charge in words and for a line that is complete; never
    /// negative, because an over-receipt is refused before it is written
    /// ([`crate::inv_po_receive`]) and the database's own CHECK backs that.
    pub fn outstanding_qty_milli(&self) -> i64 {
        if !self.is_goods() {
            return 0;
        }
        (self.line.qty_milli - self.received_qty_milli).max(0)
    }

    /// Whether everything this line asked for has arrived. A charge in words is
    /// never outstanding, so it never holds an order open.
    pub fn is_fully_received(&self) -> bool {
        self.outstanding_qty_milli() == 0
    }
}

/// A line validated by the shared rules, with its product link resolved.
#[derive(Debug)]
pub(crate) struct NormalizedPoLine {
    /// The product id as text, ready to bind; `None` for a charge in words.
    pub(crate) product_id: Option<String>,
    /// The shared line's validated fields.
    pub(crate) line: NormalizedLine,
}

/// Validates and normalises a whole line set, in the caller's order.
///
/// Two rules on top of the shared ones ([`crate::billing_line`]): a line that
/// names a product must order a **positive** quantity, because that quantity
/// becomes a movement into stock and a movement of minus four cases of wine is
/// not a purchase; and a blank product id is no product at all, since a cleared
/// picker sends `""` and means "this line is a charge in words".
///
/// The message of a rejected line names **which** line failed, 1-based as the
/// user sees it on screen, and never echoes what was typed.
///
/// # Errors
/// [`StoreError::Validation`] when the set is too long, a line breaks a shared
/// field rule, or a product line asks for a quantity that is not positive.
pub(crate) fn normalize_po_lines(lines: &[NewPoLine]) -> Result<Vec<NormalizedPoLine>> {
    let shared: Vec<NewLine> = lines.iter().map(|l| l.line.clone()).collect();
    let normalized = normalize_lines(&shared)?;
    lines
        .iter()
        .zip(normalized)
        .enumerate()
        .map(|(index, (input, line))| {
            let product_id = input
                .product_id
                .as_ref()
                .map(|id| id.as_str().trim().to_owned())
                .filter(|id| !id.is_empty());
            if product_id.is_some() && line.qty_milli <= 0 {
                return Err(StoreError::Validation(format!(
                    "line {}: a line that orders a product must ask for more than nothing",
                    index + 1
                )));
            }
            Ok(NormalizedPoLine { product_id, line })
        })
        .collect()
}

/// Every product a line set names, once each, in the order they first appear —
/// what the caller checks against this tenant's catalog before writing.
pub(crate) fn products_named(lines: &[NormalizedPoLine]) -> Vec<String> {
    let mut named: Vec<String> = Vec::new();
    for line in lines {
        if let Some(id) = line.product_id.as_ref()
            && !named.iter().any(|seen| seen == id)
        {
            named.push(id.clone());
        }
    }
    named
}

/// The lines of one order of `tenant`, in print order.
///
/// Takes any executor so the same read serves a plain pool read and a read
/// inside the transaction that holds the order's lock.
///
/// # Errors
/// [`StoreError::Db`] on failure.
pub(crate) async fn read<'e, E>(executor: E, tenant: &str, po_id: &str) -> Result<Vec<PoLine>>
where
    E: sqlx::Executor<'e, Database = sqlx::Postgres>,
{
    let rows: Vec<PoLineRow> = sqlx::query_as(
        "SELECT id, line_order, description, unit, qty_milli, unit_price_cents, vat_rate_bp, \
             product_id, received_qty_milli \
         FROM inv_purchase_order_lines WHERE tenant_id = $1 AND po_id = $2 ORDER BY line_order",
    )
    .bind(tenant)
    .bind(po_id)
    .fetch_all(executor)
    .await
    .map_err(StoreError::Db)?;
    Ok(rows.into_iter().map(PoLineRow::into_line).collect())
}

/// Replaces the whole line set of one order, in the caller's order, inside
/// `tx`: either the order reads exactly as the caller sent it or it is
/// untouched.
///
/// The lines are already validated ([`normalize_po_lines`]) and their products
/// already checked against this tenant's catalog by the caller, which also
/// holds the order's row lock. This function decides nothing about whether the
/// order may be edited.
///
/// # Errors
/// [`StoreError::Validation`] if a raised [`crate::billing_line::MAX_LINES`]
/// ever put a print position outside `i32`; [`StoreError::Db`] on failure.
pub(crate) async fn replace(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    tenant: &str,
    po_id: &str,
    lines: &[NormalizedPoLine],
) -> Result<()> {
    sqlx::query("DELETE FROM inv_purchase_order_lines WHERE tenant_id = $1 AND po_id = $2")
        .bind(tenant)
        .bind(po_id)
        .execute(&mut **tx)
        .await
        .map_err(StoreError::Db)?;

    for (index, line) in lines.iter().enumerate() {
        // Unreachable while MAX_LINES is far below i32::MAX — kept because a
        // raised cap must fail loudly here, never wrap into a negative print
        // position.
        let order = i32::try_from(index)
            .map_err(|_| StoreError::Validation("an order has too many lines".to_owned()))?;
        sqlx::query(
            "INSERT INTO inv_purchase_order_lines (tenant_id, po_id, id, line_order, \
                 product_id, description, unit, qty_milli, unit_price_cents, vat_rate_bp) \
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)",
        )
        .bind(tenant)
        .bind(po_id)
        .bind(BillingLineId::generate().as_str())
        .bind(order)
        .bind(line.product_id.as_deref())
        .bind(&line.line.description)
        .bind(&line.line.unit)
        .bind(line.line.qty_milli)
        .bind(line.line.unit_price_cents)
        .bind(line.line.vat_rate_bp)
        .execute(&mut **tx)
        .await
        .map_err(StoreError::Db)?;
    }
    Ok(())
}

/// A stored line as read back: the shared columns in [`LineRow`] order, then
/// the product link this table adds.
#[derive(sqlx::FromRow)]
struct PoLineRow {
    #[sqlx(flatten)]
    shared: LineRow,
    product_id: Option<String>,
    received_qty_milli: i64,
}

impl PoLineRow {
    fn into_line(self) -> PoLine {
        PoLine {
            product_id: self.product_id.map(BillingProductId::new),
            line: self.shared.into_line(),
            received_qty_milli: self.received_qty_milli,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn goods(qty_milli: i64) -> NewPoLine {
        NewPoLine {
            product_id: Some(BillingProductId::new("prod-1")),
            line: NewLine {
                description: "Blue chair".to_owned(),
                unit: "piece".to_owned(),
                qty_milli,
                unit_price_cents: 4_300,
                vat_rate_bp: 1900,
            },
        }
    }

    fn words(qty_milli: i64) -> NewPoLine {
        NewPoLine {
            product_id: None,
            line: NewLine {
                description: "Freight".to_owned(),
                unit: String::new(),
                qty_milli,
                unit_price_cents: 2_500,
                vat_rate_bp: 1900,
            },
        }
    }

    fn refusal(lines: &[NewPoLine]) -> String {
        match normalize_po_lines(lines) {
            Err(StoreError::Validation(message)) => message,
            other => panic!("expected a Validation refusal, got {other:?}"),
        }
    }

    #[test]
    fn a_product_line_carries_its_link_and_its_snapshot() {
        let lines = normalize_po_lines(&[goods(4_000)]).unwrap_or_else(|e| panic!("{e:?}"));
        let line = lines.first().unwrap_or_else(|| unreachable!());
        assert_eq!(line.product_id.as_deref(), Some("prod-1"));
        assert_eq!(line.line.description, "Blue chair");
        assert_eq!(line.line.qty_milli, 4_000);
        assert_eq!(line.line.unit_price_cents, 4_300);
    }

    #[test]
    fn a_line_that_orders_goods_must_order_more_than_nothing() {
        // The quantity becomes a movement into stock, and a movement of minus
        // four cases is not a purchase.
        for refused in [0, -1, -4_000] {
            let message = refusal(&[goods(refused)]);
            assert!(message.starts_with("line 1: "), "{message}");
            assert!(message.contains("more than nothing"), "{message}");
        }
        assert!(normalize_po_lines(&[goods(1)]).is_ok());
    }

    #[test]
    fn a_charge_in_words_may_be_a_discount() {
        // No product, no movement, so a negative quantity is the ordinary way
        // to write what the supplier took off — the latitude a billing line has.
        let lines = normalize_po_lines(&[words(-1_000)]).unwrap_or_else(|e| panic!("{e:?}"));
        let line = lines.first().unwrap_or_else(|| unreachable!());
        assert!(line.product_id.is_none());
        assert_eq!(line.line.qty_milli, -1_000);
    }

    #[test]
    fn a_cleared_picker_means_no_product_rather_than_a_blank_one() {
        let mut cleared = words(1_000);
        cleared.product_id = Some(BillingProductId::new("   "));
        let lines = normalize_po_lines(&[cleared]).unwrap_or_else(|e| panic!("{e:?}"));
        assert!(
            lines.first().is_some_and(|line| line.product_id.is_none()),
            "a blank id is no id, not an id that is blank"
        );
    }

    #[test]
    fn the_shared_rules_still_apply_and_name_the_line_that_broke_them() {
        let mut blank = goods(1_000);
        blank.line.description = "   ".to_owned();
        let message = refusal(&[words(1_000), blank]);
        assert!(message.starts_with("line 2: "), "{message}");

        let mut negative_price = words(1_000);
        negative_price.line.unit_price_cents = -1;
        assert!(refusal(&[negative_price]).starts_with("line 1: "));
    }

    /// A stored line, as receiving reads it back.
    fn stored(product: Option<&str>, ordered: i64, received: i64) -> PoLine {
        PoLine {
            product_id: product.map(BillingProductId::new),
            line: Line {
                id: BillingLineId::new("line-1"),
                line_order: 0,
                description: "Blue chair".to_owned(),
                unit: "piece".to_owned(),
                qty_milli: ordered,
                unit_price_cents: 4_300,
                vat_rate_bp: 1900,
            },
            received_qty_milli: received,
        }
    }

    #[test]
    fn what_is_still_to_come_is_what_was_ordered_less_what_arrived() {
        let untouched = stored(Some("prod-1"), 4_000, 0);
        assert!(untouched.is_goods());
        assert_eq!(untouched.outstanding_qty_milli(), 4_000);
        assert!(!untouched.is_fully_received());

        let part = stored(Some("prod-1"), 4_000, 2_500);
        assert_eq!(part.outstanding_qty_milli(), 1_500);
        assert!(!part.is_fully_received());

        let whole = stored(Some("prod-1"), 4_000, 4_000);
        assert_eq!(whole.outstanding_qty_milli(), 0);
        assert!(whole.is_fully_received());
    }

    #[test]
    fn a_charge_in_words_never_holds_an_order_open() {
        // Freight does not arrive on a pallet: it is not outstanding, whatever
        // its quantity says, and a negative one (a discount) cannot make the
        // outstanding figure negative either.
        for qty in [1_000, -1_000] {
            let words = stored(None, qty, 0);
            assert!(!words.is_goods());
            assert_eq!(words.outstanding_qty_milli(), 0);
            assert!(words.is_fully_received());
        }
    }

    #[test]
    fn every_product_a_set_names_is_listed_once_in_the_order_it_appears() {
        let mut second = goods(2_000);
        second.product_id = Some(BillingProductId::new("prod-2"));
        let lines = normalize_po_lines(&[goods(1_000), words(1_000), second, goods(3_000)])
            .unwrap_or_else(|e| panic!("{e:?}"));
        assert_eq!(products_named(&lines), ["prod-1", "prod-2"]);
        // A set of nothing but words names nothing, so the catalog is not read
        // at all for it.
        let words_only = normalize_po_lines(&[words(1_000)]).unwrap_or_else(|e| panic!("{e:?}"));
        assert!(products_named(&words_only).is_empty());
    }
}
