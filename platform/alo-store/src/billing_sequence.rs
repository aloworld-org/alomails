//! Legally gapless document numbering (alo Billing, ADR 0035, wave B1).
//!
//! An issued invoice must carry a number from an unbroken, per-tenant series —
//! §14 UStG in Germany and its equivalents across the EU. This module owns
//! that series: the counter and the printed form of a number, and nothing
//! else. It is a separate file from [`crate::billing_invoices`] because the
//! reason it changes is different: quotes (B1.11) and credit notes draw from
//! it too, and the format of a number is a presentation rule that outlives any
//! one document type.
//!
//! **The counter is a row, not a Postgres `SEQUENCE`.** A sequence is
//! deliberately non-transactional: a transaction that draws a number and then
//! fails burns it, leaving a permanent hole. Here the draw happens inside the
//! same transaction that writes the number onto the document, so a rollback
//! returns the number and the series stays gapless — the decision, and the
//! rejected alternative, are recorded in `docs/design/billing.md`.
//!
//! The cost of that guarantee is that issuing serialises per tenant: the
//! upsert below holds the counter's row lock until the issuing transaction
//! commits, so two concurrent issues queue rather than racing. At SME document
//! volumes that is free, and it is the only way two parallel issues can be
//! proven never to share or skip a number.

use crate::error::{Result, StoreError};

/// The series invoices and credit notes both draw from. They share it on
/// purpose: an unbroken ledger is one series, not two interleaved ones.
pub const INVOICE_SEQUENCE_KIND: &str = "invoice";

/// The prefix printed on an invoice number.
pub const INVOICE_NUMBER_PREFIX: &str = "INV";

/// The series quotes draw from (B1.11) — deliberately **not** the invoice
/// series. An offer is not a ledger entry: numbering it alongside invoices
/// would leave visible holes in the invoice series for every quote that was
/// never accepted, which is exactly the appearance gaplessness exists to
/// avoid. Quotes are numbered the same transactional way only so that no
/// customer ever receives two offers bearing one number.
pub const QUOTE_SEQUENCE_KIND: &str = "quote";

/// The prefix printed on a quote number.
pub const QUOTE_NUMBER_PREFIX: &str = "QUO";

/// The series purchase orders draw from (B5.05a2) — again its own, for the
/// quote's reason turned around: an order we place is not a document in our
/// sales ledger at all, and interleaving it with invoice numbers would put
/// visible holes in the series a tax inspector reads.
///
/// A purchase-order number is **not** legally required to be gapless — nothing
/// in §14 UStG or its equivalents says anything about it. We draw it the same
/// transactional way anyway (`docs/design/inventory.md` § The number): the
/// machinery exists, it is tested to 100 parallel iterations, and its cost —
/// serialising per tenant at the moment of numbering — is one we already pay
/// for invoices. A second, weaker numbering mechanism would only be a new thing
/// to get wrong.
pub const PURCHASE_ORDER_SEQUENCE_KIND: &str = "purchase_order";

/// The prefix printed on a purchase-order number.
pub const PURCHASE_ORDER_NUMBER_PREFIX: &str = "PO";

/// The series sales orders draw from (B5.06a) — its own again, and for the
/// quote's reason: an order a customer has confirmed is not yet a ledger entry,
/// and numbering it alongside invoices would leave a visible hole in the
/// invoice series for every order that is cancelled before it is billed.
///
/// The number matters more here than on a purchase order: it is printed on the
/// delivery note that travels in the box, so it is the thing a customer quotes
/// back when a carton is missing.
pub const SALES_ORDER_SEQUENCE_KIND: &str = "sales_order";

/// The prefix printed on a sales-order number.
pub const SALES_ORDER_NUMBER_PREFIX: &str = "SO";

/// The smallest number of digits a counter is printed with. A tenant issuing
/// more than 99 999 documents in one year simply gets a sixth digit — numbers
/// grow, they are never truncated or reused.
const COUNTER_MIN_DIGITS: usize = 5;

/// The printed form of a drawn number: `INV-2026-00042`.
///
/// The year is the calendar year of the issue date, so the counter's yearly
/// reset is visible in the number itself, and the zero padding keeps a year's
/// numbers sorting lexicographically in issue order.
///
/// # Panics
/// Never: every input has a decimal form.
pub fn document_number(prefix: &str, year: i32, value: i64) -> String {
    format!("{prefix}-{year:04}-{value:0COUNTER_MIN_DIGITS$}")
}

/// Draws the next number of `kind` for this tenant and year, inside `tx`.
///
/// The single statement both creates the series on first use (at 2, taking 1)
/// and advances it, and it holds the row's lock until `tx` ends: a concurrent
/// issue for the same tenant, kind and year blocks here and then draws the
/// following number, while a `tx` that rolls back gives its number back.
///
/// # Errors
/// [`StoreError::Db`] on failure — including the checked bounds of the table,
/// which the caller's own validation should already have made unreachable.
pub(crate) async fn draw_next(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    tenant: &str,
    kind: &str,
    year: i32,
) -> Result<i64> {
    let drawn: i64 = sqlx::query_scalar(
        "INSERT INTO billing_sequences AS s (tenant_id, kind, year, next_value) \
         VALUES ($1, $2, $3, 2) \
         ON CONFLICT (tenant_id, kind, year) \
         DO UPDATE SET next_value = s.next_value + 1, updated_at = now() \
         RETURNING s.next_value - 1",
    )
    .bind(tenant)
    .bind(kind)
    .bind(year)
    .fetch_one(&mut **tx)
    .await
    .map_err(StoreError::Db)?;
    Ok(drawn)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_number_prints_as_prefix_year_and_padded_counter() {
        assert_eq!(
            document_number(INVOICE_NUMBER_PREFIX, 2026, 1),
            "INV-2026-00001"
        );
        assert_eq!(
            document_number(INVOICE_NUMBER_PREFIX, 2026, 42),
            "INV-2026-00042"
        );
        assert_eq!(
            document_number(INVOICE_NUMBER_PREFIX, 2027, 99_999),
            "INV-2027-99999"
        );
    }

    #[test]
    fn a_year_of_more_than_ninety_nine_thousand_documents_grows_the_number() {
        // Truncating or wrapping would produce a duplicate number, which is
        // exactly what the series exists to prevent. Six digits it is.
        assert_eq!(
            document_number(INVOICE_NUMBER_PREFIX, 2026, 100_000),
            "INV-2026-100000"
        );
        assert_eq!(
            document_number(INVOICE_NUMBER_PREFIX, 2026, 1_234_567),
            "INV-2026-1234567"
        );
    }

    #[test]
    fn numbers_of_one_year_sort_in_issue_order_as_text() {
        // The list surface and every export sort on the number as a string, so
        // the padding is load-bearing, not decoration.
        let mut printed: Vec<String> = [7, 100, 2, 99, 1]
            .into_iter()
            .map(|value| document_number(INVOICE_NUMBER_PREFIX, 2026, value))
            .collect();
        printed.sort();
        assert_eq!(
            printed,
            [
                "INV-2026-00001",
                "INV-2026-00002",
                "INV-2026-00007",
                "INV-2026-00099",
                "INV-2026-00100",
            ]
        );
    }

    #[test]
    fn quotes_and_invoices_are_told_apart_by_prefix_and_by_series() {
        // Same format, different series: a quote number can never be mistaken
        // for an invoice number on a document, and an unaccepted quote can
        // never leave a hole in the invoice series.
        assert_eq!(
            document_number(QUOTE_NUMBER_PREFIX, 2026, 1),
            "QUO-2026-00001"
        );
        assert_ne!(QUOTE_SEQUENCE_KIND, INVOICE_SEQUENCE_KIND);
        // An order we place is a third series, for the same reason: it must
        // leave no hole in either of the two a customer ever sees.
        assert_eq!(
            document_number(PURCHASE_ORDER_NUMBER_PREFIX, 2026, 1),
            "PO-2026-00001"
        );
        assert_ne!(PURCHASE_ORDER_SEQUENCE_KIND, INVOICE_SEQUENCE_KIND);
        assert_ne!(PURCHASE_ORDER_SEQUENCE_KIND, QUOTE_SEQUENCE_KIND);
        // And an order a customer places with us is a fourth: cancelling one
        // before it is billed must leave no hole in the invoice series.
        assert_eq!(
            document_number(SALES_ORDER_NUMBER_PREFIX, 2026, 1),
            "SO-2026-00001"
        );
        for other in [
            INVOICE_SEQUENCE_KIND,
            QUOTE_SEQUENCE_KIND,
            PURCHASE_ORDER_SEQUENCE_KIND,
        ] {
            assert_ne!(SALES_ORDER_SEQUENCE_KIND, other);
        }
        // Every kind satisfies the table's shape check (lowercase, ≤ 32 chars),
        // which is what lets a new series be a row rather than a migration.
        for kind in [
            INVOICE_SEQUENCE_KIND,
            QUOTE_SEQUENCE_KIND,
            PURCHASE_ORDER_SEQUENCE_KIND,
            SALES_ORDER_SEQUENCE_KIND,
        ] {
            assert!(
                !kind.is_empty()
                    && kind.len() <= 32
                    && kind.bytes().all(|b| b.is_ascii_lowercase() || b == b'_'),
                "{kind:?} must satisfy billing_sequences_kind_shape"
            );
        }
    }

    #[test]
    fn the_year_is_always_four_digits() {
        assert_eq!(document_number("INV", 999, 1), "INV-0999-00001");
        assert_eq!(document_number("INV", 10_000, 1), "INV-10000-00001");
    }
}
