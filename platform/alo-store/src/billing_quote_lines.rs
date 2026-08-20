//! The line of a **quote**, which — alone among billing documents — may name the
//! catalog item it is selling (alo Orders, ADR 0054 §5, migration 0701).
//!
//! ## Why a quote's line is not simply a billing line
//!
//! An invoice, a bill and a recurring template are money documents: what they
//! say is a description, a quantity, a price and a rate, and
//! [`crate::billing_line`] is exactly that. **A quote can become a sales
//! order**, and an order line that names no product is one nothing can ever be
//! delivered against — `inv_so_deliver` refuses it as *"a charge in words, not
//! goods"*. So the offer is the one document that has to record *what* it is
//! selling, and this module is that difference and nothing else.
//!
//! The shape is `inv_so_lines`' own: a product beside the shared line rather
//! than a second line model. That keeps a quote line and an invoice line the
//! same thing where they are the same thing — which is what makes copying an
//! accepted quote onto an invoice draft a copy rather than a translation.
//!
//! ## The snapshot is untouched
//!
//! Migration 0105 says a line snapshots the price list so a later price change
//! never rewrites an offer already made, and **that stays exactly true**.
//! `description`, `unit`, `unit_price_cents` and `vat_rate_bp` are still the
//! frozen copy; nothing here reads the product to price anything. The product is
//! *provenance* — which of our items this line is — and it is the same
//! distinction migration 0700 drew for the order-to-quote link.

use crate::billing_line::{Line, LineRow, NewLine, NormalizedLine, normalize_lines};
use crate::error::{Result, StoreError};
use crate::id::{BillingLineId, BillingProductId};

/// A stored quote line: the catalog item it sells, and the shared line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QuoteLine {
    /// The item this line sells, or `None` for a charge in words — assembly,
    /// delivery, a discount. Also `None` once a named product has been deleted
    /// from the catalog, which leaves the offer readable exactly as it was made.
    pub product_id: Option<BillingProductId>,
    /// The line as every billing document states it.
    pub line: Line,
}

/// A quote line as a caller states it.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct NewQuoteLine {
    /// The item being sold, or `None` for a charge in words.
    pub product_id: Option<BillingProductId>,
    /// The shared line body.
    pub line: NewLine,
}

impl From<NewLine> for NewQuoteLine {
    /// A line in words — the overwhelmingly common offer, and every offer made
    /// before migration 0701 existed. Lets a caller that has nothing to say
    /// about the catalog go on writing plain lines.
    fn from(line: NewLine) -> Self {
        Self {
            product_id: None,
            line,
        }
    }
}

/// A validated quote line, ready to be written.
pub(crate) struct NormalizedQuoteLine {
    pub(crate) product_id: Option<String>,
    pub(crate) line: NormalizedLine,
}

/// Validates a whole set, in the caller's order, with the shared field rules.
///
/// A blank product id is **no product at all** — a cleared picker sends `""` —
/// and whether the id is *ours* is decided by the caller that owns the catalog,
/// never here.
///
/// # Errors
/// [`StoreError::Validation`] naming the 1-based line that broke a rule.
pub(crate) fn normalize_quote_lines(lines: &[NewQuoteLine]) -> Result<Vec<NormalizedQuoteLine>> {
    let bodies: Vec<NewLine> = lines.iter().map(|l| l.line.clone()).collect();
    let normalized = normalize_lines(&bodies)?;
    Ok(lines
        .iter()
        .zip(normalized)
        .map(|(stated, line)| NormalizedQuoteLine {
            product_id: stated
                .product_id
                .as_ref()
                .map(|id| id.as_str().trim().to_owned())
                .filter(|id| !id.is_empty()),
            line,
        })
        .collect())
}

/// Every product a line set names, deduplicated — what a caller holds to its own
/// catalog before writing.
pub(crate) fn products_named(lines: &[NormalizedQuoteLine]) -> Vec<String> {
    let mut named: Vec<String> = lines.iter().filter_map(|l| l.product_id.clone()).collect();
    named.sort();
    named.dedup();
    named
}

/// The lines of one quote, in print order.
///
/// # Errors
/// [`StoreError::Db`] on failure.
pub(crate) async fn read<'e, E>(executor: E, tenant: &str, quote_id: &str) -> Result<Vec<QuoteLine>>
where
    E: sqlx::Executor<'e, Database = sqlx::Postgres>,
{
    let rows: Vec<QuoteLineRow> = sqlx::query_as(
        "SELECT id, line_order, description, unit, qty_milli, unit_price_cents, vat_rate_bp, \
             product_id \
         FROM billing_quote_lines \
         WHERE tenant_id = $1 AND quote_id = $2 ORDER BY line_order",
    )
    .bind(tenant)
    .bind(quote_id)
    .fetch_all(executor)
    .await
    .map_err(StoreError::Db)?;
    Ok(rows.into_iter().map(QuoteLineRow::into_line).collect())
}

/// Replaces the whole line set of one quote inside `tx`: the offer reads exactly
/// as the caller sent it, or it is untouched.
///
/// # Errors
/// [`StoreError::Db`] on failure.
pub(crate) async fn replace(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    tenant: &str,
    quote_id: &str,
    lines: &[NormalizedQuoteLine],
) -> Result<()> {
    sqlx::query("DELETE FROM billing_quote_lines WHERE tenant_id = $1 AND quote_id = $2")
        .bind(tenant)
        .bind(quote_id)
        .execute(&mut **tx)
        .await
        .map_err(StoreError::Db)?;
    for (index, line) in lines.iter().enumerate() {
        let order = i32::try_from(index)
            .map_err(|_| StoreError::Validation("a document has too many lines".to_owned()))?;
        write(tx, tenant, quote_id, order, line).await?;
    }
    Ok(())
}

/// Writes one line at `order`, with an id of its own.
async fn write(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    tenant: &str,
    quote_id: &str,
    order: i32,
    line: &NormalizedQuoteLine,
) -> Result<()> {
    sqlx::query(
        "INSERT INTO billing_quote_lines (tenant_id, quote_id, id, line_order, description, \
             unit, qty_milli, unit_price_cents, vat_rate_bp, product_id) \
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)",
    )
    .bind(tenant)
    .bind(quote_id)
    .bind(BillingLineId::generate().as_str())
    .bind(order)
    .bind(&line.line.description)
    .bind(&line.line.unit)
    .bind(line.line.qty_milli)
    .bind(line.line.unit_price_cents)
    .bind(line.line.vat_rate_bp)
    .bind(line.product_id.as_deref())
    .execute(&mut **tx)
    .await
    .map_err(StoreError::Db)?;
    Ok(())
}

/// A stored quote line as read back.
#[derive(sqlx::FromRow)]
struct QuoteLineRow {
    #[sqlx(flatten)]
    line: LineRow,
    product_id: Option<String>,
}

impl QuoteLineRow {
    fn into_line(self) -> QuoteLine {
        QuoteLine {
            product_id: self.product_id.map(BillingProductId::new),
            line: self.line.into_line(),
        }
    }
}
