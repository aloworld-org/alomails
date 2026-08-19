//! The order book (alo Orders, item O1.d) — what has been promised, what has
//! gone out, what has been billed, and what is still owed, per order and in
//! total.
//!
//! **This is a read and nothing else.** Every number here is folded from
//! quantities that already exist on the lines — `qty_milli`,
//! `delivered_qty_milli`, `invoiced_qty_milli` — at the moment of asking. There
//! is no stored total anywhere in it, for the reason `inv_so` gives about money
//! generally: a figure kept beside the lines that justify it is a figure that
//! will one day disagree with them.
//!
//! ## The money is the document's own arithmetic, at a different quantity
//!
//! Delivered and invoiced value are **not** a proportion of the order's total.
//! Splitting a rounded total by a ratio produces cents that belong to nothing,
//! and the parts stop adding up to the whole. Each is
//! [`crate::billing_totals::line_net_cents`] — the same function the order, the
//! invoice and the quote all use — applied to the same line at the quantity that
//! actually went out or was actually billed. So a line delivered in full
//! contributes exactly what it contributes to the order, to the cent, and
//! `outstanding = ordered − delivered` needs no reconciliation.
//!
//! ## What "reserved" means here, since ADR 0054 made it computed
//!
//! There is no reserved column and this module does not invent one. What an
//! order holds against the warehouse is its **undelivered remainder while it is
//! open** — the same fold `inv_reorder`'s `committed` performs across all orders,
//! seen one order at a time. A draft reserves nothing because nobody has been
//! promised anything; a delivered or cancelled order reserves nothing because
//! there is nothing left to send. That is `reserved`, and it is stated in
//! quantity **and** in money because a manufacturer asks both questions.
//!
//! ## Charges in words are counted in the money and not in the quantity
//!
//! A line with no product — assembly, delivery, a discount — has a value but
//! never leaves on a pallet, and `inv_so_deliver` refuses to move it. Its
//! `delivered_qty_milli` is therefore always zero, and counting it in an
//! outstanding *quantity* would hold an order open for ever. It counts in the
//! money, where it is real, and is excluded from the quantities, where it is not.

use crate::account::AccountStore;
use crate::billing_totals::{LineFigures, line_net_cents};
use crate::error::{Result, StoreError};
use crate::id::{BillingCustomerId, InvSalesOrderId};
use crate::inv_so::SoStatus;

/// One order's five numbers, in money and in goods.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OrderBookRow {
    /// The order.
    pub id: InvSalesOrderId,
    /// Its number, `None` while it is a draft.
    pub number: Option<String>,
    /// Who it is for.
    pub customer_id: BillingCustomerId,
    /// Their name as it stands now — a list read by a person.
    pub customer_name: String,
    /// Where the order is in its life.
    pub status: SoStatus,
    /// The currency the order was taken in.
    pub currency: String,
    /// The five figures.
    pub figures: BookFigures,
}

/// Ordered, reserved, delivered, invoiced and outstanding — the five numbers the
/// wave is named after, each in cents and (where goods can move) in milli-units.
///
/// `outstanding` is `ordered − delivered`: what the customer is still waiting
/// for. It is deliberately not `ordered − invoiced`, which is a money question
/// about what is still to bill and is answered by `invoiced` beside it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct BookFigures {
    /// Net value of everything on the order.
    pub ordered_net_cents: i64,
    /// Net value still to go out, and zero unless the order is open — what this
    /// order is holding against the warehouse.
    pub reserved_net_cents: i64,
    /// Net value of what has left the building.
    pub delivered_net_cents: i64,
    /// Net value of what has been billed.
    pub invoiced_net_cents: i64,
    /// Net value still to go out, whatever the order's state.
    pub outstanding_net_cents: i64,
    /// Ordered quantity of goods, in milli-units. Charges in words are excluded:
    /// they are value, not goods.
    pub ordered_qty_milli: i64,
    /// Quantity still to go out while the order is open; zero otherwise.
    pub reserved_qty_milli: i64,
    /// Quantity that has left.
    pub delivered_qty_milli: i64,
    /// Quantity still to go out, whatever the state.
    pub outstanding_qty_milli: i64,
}

/// The book as a whole: every order that matters, and the totals under them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OrderBook {
    /// The orders, newest first.
    pub rows: Vec<OrderBookRow>,
    /// The sum of the rows' figures.
    ///
    /// **Only meaningful when the rows share a currency**, which is why
    /// [`OrderBook::currencies`] is beside it: a screen that shows one total over
    /// two currencies is showing a number that means nothing, and it should show
    /// the rows instead.
    pub totals: BookFigures,
    /// Every currency present in the rows, sorted. One entry is the ordinary
    /// case; two or more is the signal not to print `totals` as one figure.
    pub currencies: Vec<String>,
}

/// Which orders the book covers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum BookScope {
    /// The orders that are still going somewhere: `confirmed` and
    /// `partially_delivered`. **The default, and the screen a manufacturer opens
    /// in the morning** — a book cluttered with finished business answers a
    /// question nobody asked.
    #[default]
    Open,
    /// Every order the tenant has, drafts and closed ones included, for the
    /// reader who is looking for one in particular.
    All,
}

/// One line, as the book needs it: its figures and how much of it has moved.
#[derive(sqlx::FromRow)]
struct BookLineRow {
    so_id: String,
    product_id: Option<String>,
    qty_milli: i64,
    delivered_qty_milli: i64,
    invoiced_qty_milli: i64,
    unit_price_cents: i64,
    vat_rate_bp: i32,
}

/// One order's header, as the book needs it.
#[derive(sqlx::FromRow)]
struct BookOrderRow {
    id: String,
    number: Option<String>,
    customer_id: String,
    customer_name: Option<String>,
    status: String,
    currency: String,
}

impl BookLineRow {
    /// The net of this line at an arbitrary quantity, using the document's own
    /// rounding. Not a share of the line's total — the same function, one input
    /// changed.
    fn net_at(&self, qty_milli: i64) -> i64 {
        line_net_cents(&LineFigures {
            qty_milli,
            unit_price_cents: self.unit_price_cents,
            vat_rate_bp: self.vat_rate_bp,
        })
    }

    /// What is still to go out on this line: `ordered − delivered`, clamped so it
    /// can never overshoot **past zero**, in whichever direction the line runs.
    ///
    /// The clamp is sign-aware and that is not fussiness. An over-delivered
    /// positive line must read `0` left rather than a negative that a reader
    /// would take for a credit — over-delivery is refused at the door, so it is a
    /// stored inconsistency rather than a state. But a **negative quantity is
    /// legitimate**: it is how `billing_line` expresses a discount, and clamping
    /// it at zero would silently drop the discount from the book and overstate
    /// what the customer owes. A unit test caught exactly that.
    fn outstanding_qty(&self) -> i64 {
        let left = self.qty_milli.saturating_sub(self.delivered_qty_milli);
        if self.qty_milli >= 0 {
            left.max(0)
        } else {
            left.min(0)
        }
    }

    /// Whether goods can move against this line at all.
    fn is_goods(&self) -> bool {
        self.product_id.is_some()
    }
}

impl BookFigures {
    /// Folds one line into the figures, given whether its order is open.
    fn add(&mut self, line: &BookLineRow, open: bool) {
        let outstanding_qty = line.outstanding_qty();
        let ordered_net = line.net_at(line.qty_milli);
        let delivered_net = line.net_at(line.delivered_qty_milli);
        let outstanding_net = line.net_at(outstanding_qty);

        self.ordered_net_cents += ordered_net;
        self.delivered_net_cents += delivered_net;
        self.invoiced_net_cents += line.net_at(line.invoiced_qty_milli);
        self.outstanding_net_cents += outstanding_net;
        if open {
            self.reserved_net_cents += outstanding_net;
        }

        // Goods only: a charge in words never leaves on a pallet, and counting
        // it here would hold an order open in the quantity column for ever.
        //
        // A negative quantity contributes nothing to the goods columns even on a
        // line that names a product — `inv_reorder`'s `committed` fold says the
        // same thing with `GREATEST(qty, 0)`, and a book that disagreed with the
        // shortage report about what is promised would be the second truth about
        // one shelf this wave exists to avoid.
        if line.is_goods() {
            self.ordered_qty_milli += line.qty_milli.max(0);
            self.delivered_qty_milli += line.delivered_qty_milli;
            self.outstanding_qty_milli += outstanding_qty.max(0);
            if open {
                self.reserved_qty_milli += outstanding_qty.max(0);
            }
        }
    }

    /// Adds another row's figures into these — how the book's totals are made.
    fn absorb(&mut self, other: &Self) {
        self.ordered_net_cents += other.ordered_net_cents;
        self.reserved_net_cents += other.reserved_net_cents;
        self.delivered_net_cents += other.delivered_net_cents;
        self.invoiced_net_cents += other.invoiced_net_cents;
        self.outstanding_net_cents += other.outstanding_net_cents;
        self.ordered_qty_milli += other.ordered_qty_milli;
        self.reserved_qty_milli += other.reserved_qty_milli;
        self.delivered_qty_milli += other.delivered_qty_milli;
        self.outstanding_qty_milli += other.outstanding_qty_milli;
    }
}

impl AccountStore {
    /// The order book: every order in `scope`, newest first, with its five
    /// figures and the totals under them.
    ///
    /// Two statements, never one per order: the headers, then every line of
    /// those headers in one go. An order book on a busy tenant is the screen
    /// most likely to be opened at nine in the morning by everybody at once.
    ///
    /// # Errors
    /// [`StoreError::Db`] on failure, and on a stored status this build does not
    /// know — a row we cannot classify must not be silently counted as closed
    /// and quietly drop out of the reserved column.
    pub async fn inv_order_book(&self, scope: BookScope) -> Result<OrderBook> {
        let open_only = matches!(scope, BookScope::Open);
        let headers = sqlx::query_as::<_, BookOrderRow>(
            "SELECT o.id, o.number, o.customer_id, o.status, o.currency, \
                 (SELECT name FROM billing_customers c \
                   WHERE c.tenant_id = o.tenant_id AND c.id = o.customer_id) AS customer_name \
             FROM inv_sales_orders o \
             WHERE o.tenant_id = $1 \
               AND ($2 = FALSE OR o.status IN ('confirmed', 'partially_delivered')) \
             ORDER BY o.created_at DESC, o.id",
        )
        .bind(self.tenant.as_str())
        .bind(open_only)
        .fetch_all(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        if headers.is_empty() {
            return Ok(OrderBook {
                rows: Vec::new(),
                totals: BookFigures::default(),
                currencies: Vec::new(),
            });
        }

        // One statement for every line of every listed order. `invoiced` is
        // `inv_so_lines`' own correlated sum, spliced in rather than restated:
        // a second reading of what has been billed would let this book and the
        // order document disagree about one line.
        let ids: Vec<String> = headers.iter().map(|h| h.id.clone()).collect();
        let lines = sqlx::query_as::<_, BookLineRow>(&format!(
            "SELECT l.so_id, l.product_id, l.qty_milli, l.delivered_qty_milli, \
                 l.unit_price_cents, l.vat_rate_bp, \
                 {} AS invoiced_qty_milli \
             FROM inv_sales_order_lines l \
             WHERE l.tenant_id = $1 AND l.so_id = ANY($2)",
            crate::inv_so_lines::INVOICED_QTY_SQL
        ))
        .bind(self.tenant.as_str())
        .bind(&ids)
        .fetch_all(&self.pool)
        .await
        .map_err(StoreError::Db)?;

        let mut rows = Vec::with_capacity(headers.len());
        let mut totals = BookFigures::default();
        let mut currencies: Vec<String> = Vec::new();
        for header in &headers {
            let status = SoStatus::parse(&header.status).ok_or_else(|| {
                StoreError::Db(sqlx::Error::Decode(
                    "inv_sales_orders.status is not a known status".into(),
                ))
            })?;
            let open = status.is_open();
            let mut figures = BookFigures::default();
            for line in lines.iter().filter(|l| l.so_id == header.id) {
                figures.add(line, open);
            }
            totals.absorb(&figures);
            if !currencies.contains(&header.currency) {
                currencies.push(header.currency.clone());
            }
            rows.push(OrderBookRow {
                id: InvSalesOrderId::new(header.id.clone()),
                number: header.number.clone(),
                customer_id: BillingCustomerId::new(header.customer_id.clone()),
                customer_name: header.customer_name.clone().unwrap_or_default(),
                status,
                currency: header.currency.clone(),
                figures,
            });
        }
        currencies.sort();
        Ok(OrderBook {
            rows,
            totals,
            currencies,
        })
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;

    fn goods(qty: i64, delivered: i64, invoiced: i64, price: i64) -> BookLineRow {
        BookLineRow {
            so_id: "so".to_owned(),
            product_id: Some("prod".to_owned()),
            qty_milli: qty,
            delivered_qty_milli: delivered,
            invoiced_qty_milli: invoiced,
            unit_price_cents: price,
            vat_rate_bp: 2100,
        }
    }

    fn charge(qty: i64, price: i64) -> BookLineRow {
        BookLineRow {
            product_id: None,
            ..goods(qty, 0, 0, price)
        }
    }

    #[test]
    fn the_parts_add_up_to_the_whole_to_the_cent() {
        // The reason delivered value is the line's own arithmetic at a different
        // quantity rather than a share of its total: a third of 100 cents split
        // by ratio leaves a cent belonging to nothing.
        let line = goods(3_000, 1_000, 0, 100);
        let mut figures = BookFigures::default();
        figures.add(&line, true);
        assert_eq!(figures.ordered_net_cents, 300);
        assert_eq!(figures.delivered_net_cents, 100);
        assert_eq!(figures.outstanding_net_cents, 200);
        assert_eq!(
            figures.delivered_net_cents + figures.outstanding_net_cents,
            figures.ordered_net_cents,
            "delivered and outstanding must reconstitute the order exactly"
        );
    }

    #[test]
    fn an_open_order_reserves_its_remainder_and_a_finished_one_reserves_nothing() {
        let line = goods(6_000, 2_000, 2_000, 12_950);
        let mut open = BookFigures::default();
        open.add(&line, true);
        assert_eq!(open.reserved_qty_milli, 4_000);
        assert_eq!(open.reserved_net_cents, open.outstanding_net_cents);

        let mut closed = BookFigures::default();
        closed.add(&line, false);
        assert_eq!(closed.reserved_qty_milli, 0);
        assert_eq!(closed.reserved_net_cents, 0);
        // What is still outstanding is a fact about the order either way — only
        // what it *holds against the warehouse* depends on it being open.
        assert_eq!(closed.outstanding_qty_milli, 4_000);
        assert_eq!(closed.outstanding_net_cents, open.outstanding_net_cents);
    }

    #[test]
    fn a_charge_in_words_is_money_and_never_goods() {
        // Assembly has a value and never leaves on a pallet. Counted in the
        // quantity it would hold the order open for ever, because
        // `inv_so_deliver` refuses to move it.
        let mut figures = BookFigures::default();
        figures.add(&charge(1_000, 9_500), true);
        assert_eq!(figures.ordered_net_cents, 9_500);
        assert_eq!(figures.outstanding_net_cents, 9_500);
        assert_eq!(
            figures.ordered_qty_milli, 0,
            "a charge in words is not a quantity of goods"
        );
        assert_eq!(figures.outstanding_qty_milli, 0);
        assert_eq!(figures.reserved_qty_milli, 0);
    }

    #[test]
    fn what_is_billed_is_read_from_the_line_and_not_guessed_from_what_shipped() {
        // Delivered and invoiced move independently: goods can go out and be
        // billed later, and a deposit can be billed before anything ships.
        let mut shipped_unbilled = BookFigures::default();
        shipped_unbilled.add(&goods(4_000, 4_000, 0, 8_600), false);
        assert_eq!(shipped_unbilled.delivered_net_cents, 34_400);
        assert_eq!(shipped_unbilled.invoiced_net_cents, 0);

        let mut billed_unshipped = BookFigures::default();
        billed_unshipped.add(&goods(4_000, 0, 4_000, 8_600), true);
        assert_eq!(billed_unshipped.delivered_net_cents, 0);
        assert_eq!(billed_unshipped.invoiced_net_cents, 34_400);
        assert_eq!(
            billed_unshipped.outstanding_net_cents, 34_400,
            "billing something does not deliver it"
        );
    }

    #[test]
    fn an_over_delivered_line_never_prints_a_negative_outstanding() {
        // Over-delivery is refused at the door, so this is a stored
        // inconsistency rather than an ordinary state — and the book must report
        // zero left rather than a negative that reads as a credit.
        let mut figures = BookFigures::default();
        figures.add(&goods(1_000, 3_000, 0, 500), true);
        assert_eq!(figures.outstanding_qty_milli, 0);
        assert_eq!(figures.outstanding_net_cents, 0);
        assert_eq!(figures.reserved_qty_milli, 0);
    }

    #[test]
    fn totals_are_the_rows_added_up_and_nothing_else() {
        let mut first = BookFigures::default();
        first.add(&goods(2_000, 1_000, 1_000, 1_000), true);
        let mut second = BookFigures::default();
        second.add(&goods(5_000, 0, 0, 250), true);

        let mut totals = BookFigures::default();
        totals.absorb(&first);
        totals.absorb(&second);
        assert_eq!(
            totals.ordered_net_cents,
            first.ordered_net_cents + second.ordered_net_cents
        );
        assert_eq!(totals.ordered_qty_milli, 7_000);
        assert_eq!(totals.reserved_qty_milli, 6_000);
        assert_eq!(totals.delivered_net_cents, 1_000);
    }

    #[test]
    fn a_discount_line_lowers_the_book_rather_than_being_dropped() {
        // A negative quantity is how a discount is expressed (billing_line), and
        // an order book that silently ignored it would overstate what the
        // customer owes.
        let mut figures = BookFigures::default();
        figures.add(&charge(-1_000, 5_000), true);
        assert_eq!(figures.ordered_net_cents, -5_000);
        assert_eq!(
            figures.outstanding_net_cents, -5_000,
            "an undelivered discount is still owed to the customer"
        );
    }
}
