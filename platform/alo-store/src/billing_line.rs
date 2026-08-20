//! The line of a billing document (alo Billing, ADR 0035, wave B1) — shared
//! by invoices and, from B1.11, by quotes.
//!
//! "Shared" is meant literally: the two documents keep their own tables
//! (`billing_invoice_lines`, `billing_quote_lines`, which differ only in the
//! column naming their document), and this module owns the one line model,
//! the one set of field rules, and the one statement that writes a line
//! ([`LineTable`]). A quote's line and an invoice's line are the same thing —
//! which is what makes copying an accepted quote onto an invoice draft (B1.12)
//! a copy rather than a translation.
//!
//! A line is a **snapshot**, not a reference. Picking a product copies its
//! description, unit, price and VAT rate onto the line at that moment; there
//! is no foreign key back to the price list, because editing a price must
//! never rewrite a document that was already raised (`docs/design/billing.md`).
//! That is the whole reason this type exists instead of a product id and a
//! quantity.
//!
//! Quantities are **milli-units** (1.5 h = 1500) so a third of an hour or a
//! kilo-and-a-bit is exact; prices are integer cents and rates basis points.
//! A negative quantity is legitimate — it is how a discount line is written —
//! while a negative *price* is not (see [`crate::billing_field`]).
//!
//! The bounds here are what make the arithmetic in [`crate::billing_totals`]
//! provably safe: |qty| ≤ 10^9 milli-units, price ≤ 10^9 cents and
//! [`MAX_LINES`] lines put a document's gross four orders of magnitude below
//! `i64::MAX`.

use std::collections::HashMap;

use crate::billing_field::{bounded, required, unit_price_cents, vat_rate_bp};
use crate::billing_totals::LineFigures;
use crate::error::{Result, StoreError};
use crate::id::BillingLineId;

/// A line description is what the customer reads; it is the product name
/// copied over, so it carries the same bound
/// ([`crate::billing_products::PRODUCT_NAME_MAX_CHARS`]) and a line can never
/// truncate the item it was raised from.
pub const LINE_DESCRIPTION_MAX_CHARS: usize = 200;
/// A unit label is a word, not a sentence — same bound as the price list.
pub const LINE_UNIT_MAX_CHARS: usize = 32;
/// The largest quantity a line may carry, in milli-units: a million units.
/// Beyond that it is a typo, and the cap is what keeps
/// `qty × price` inside `i64`.
pub const QTY_MAX_MILLI: i64 = 1_000_000_000;
/// The most lines one document may carry. A real invoice with more than this
/// is an export, not a document a human reads; the cap also bounds the sum in
/// [`crate::billing_totals::totals`].
pub const MAX_LINES: usize = 500;

/// The writable shape of a line. A document's lines are always written as a
/// whole set, in the caller's order, so a line carries no id or position of
/// its own on the way in.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct NewLine {
    /// What the customer reads. Required, non-blank.
    pub description: String,
    /// Unit label ("hour", "piece"); empty for a unitless line.
    pub unit: String,
    /// Quantity in milli-units (1.5 = 1500); negative for a discount line.
    pub qty_milli: i64,
    /// Price of one unit in integer cents, snapshotted from the price list.
    pub unit_price_cents: i64,
    /// VAT rate in basis points (2100 = 21 %), snapshotted likewise.
    pub vat_rate_bp: i32,
}

/// A stored line of a document.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Line {
    /// Opaque id, unique within the tenant.
    pub id: BillingLineId,
    /// Position on the printed document, 0-based.
    pub line_order: i32,
    /// What the customer reads.
    pub description: String,
    /// Unit label; empty for a unitless line.
    pub unit: String,
    /// Quantity in milli-units.
    pub qty_milli: i64,
    /// Price of one unit in integer cents, as snapshotted.
    pub unit_price_cents: i64,
    /// VAT rate in basis points, as snapshotted.
    pub vat_rate_bp: i32,
}

impl Line {
    /// The three numbers this line contributes to the document's totals.
    pub fn figures(&self) -> LineFigures {
        LineFigures {
            qty_milli: self.qty_milli,
            unit_price_cents: self.unit_price_cents,
            vat_rate_bp: self.vat_rate_bp,
        }
    }

    /// This line's net, in cents — quantity times price, rounded once.
    /// VAT is not a per-line number: it is rounded at the rate subtotal
    /// ([`crate::billing_totals`]).
    pub fn net_cents(&self) -> i64 {
        crate::billing_totals::line_net_cents(&self.figures())
    }
}

/// A validated, normalised line ready to be bound into a statement.
#[derive(Debug)]
pub(crate) struct NormalizedLine {
    pub(crate) description: String,
    pub(crate) unit: String,
    pub(crate) qty_milli: i64,
    pub(crate) unit_price_cents: i64,
    pub(crate) vat_rate_bp: i32,
}

impl NormalizedLine {
    /// The figures of a line that has not been stored yet — so a caller can
    /// compute what a document *would* total before writing it.
    pub(crate) fn figures(&self) -> LineFigures {
        LineFigures {
            qty_milli: self.qty_milli,
            unit_price_cents: self.unit_price_cents,
            vat_rate_bp: self.vat_rate_bp,
        }
    }
}

/// Validates a quantity in milli-units. Negative is allowed (a discount);
/// the magnitude is capped so the totals arithmetic cannot overflow.
fn qty_milli(value: i64) -> Result<i64> {
    if !(-QTY_MAX_MILLI..=QTY_MAX_MILLI).contains(&value) {
        return Err(StoreError::Validation(format!(
            "quantity must be between {} and {QTY_MAX_MILLI} milli-units",
            -QTY_MAX_MILLI
        )));
    }
    Ok(value)
}

/// Validates and normalises one line. Pure — no database, so the rules are
/// unit-tested directly.
fn normalize_line(input: &NewLine) -> Result<NormalizedLine> {
    Ok(NormalizedLine {
        description: required(
            "description",
            &input.description,
            LINE_DESCRIPTION_MAX_CHARS,
        )?,
        unit: bounded("unit", &input.unit, LINE_UNIT_MAX_CHARS)?,
        qty_milli: qty_milli(input.qty_milli)?,
        unit_price_cents: unit_price_cents("unit price", input.unit_price_cents)?,
        vat_rate_bp: vat_rate_bp(input.vat_rate_bp)?,
    })
}

/// Validates and normalises a whole line set, in the caller's order.
///
/// The message of a rejected line names **which** line failed, 1-based as the
/// user sees it on screen, so a fifty-line document does not have to be hunted
/// through by hand. It still never echoes the value itself (law 1).
///
/// # Errors
/// [`StoreError::Validation`] when the document has more than [`MAX_LINES`]
/// lines, or any line breaks a field rule.
pub(crate) fn normalize_lines(lines: &[NewLine]) -> Result<Vec<NormalizedLine>> {
    if lines.len() > MAX_LINES {
        return Err(StoreError::Validation(format!(
            "a document may have at most {MAX_LINES} lines"
        )));
    }
    lines
        .iter()
        .enumerate()
        .map(|(index, line)| {
            normalize_line(line).map_err(|error| match error {
                StoreError::Validation(rule) => {
                    StoreError::Validation(format!("line {}: {rule}", index + 1))
                }
                other => other,
            })
        })
        .collect()
}

// ---- storage ----------------------------------------------------------------

/// The columns every read of a line selects, in [`LineRow`] order.
pub(crate) const LINE_COLS: &str = "id, line_order, description, unit, qty_milli, \
     unit_price_cents, vat_rate_bp";

/// One of the tables a line set lives in, and the column naming the document
/// each line belongs to.
///
/// Invoices and quotes keep separate tables — they are separate documents with
/// separate lives — but the statements over those tables differ only in these
/// two identifiers, so they are written once here rather than copied. Both
/// fields are compile-time constants of this crate; no caller-supplied text
/// ever reaches a statement built from them.
#[derive(Debug, Clone, Copy)]
pub(crate) struct LineTable {
    table: &'static str,
    doc_column: &'static str,
}

/// The lines of an invoice ([`crate::billing_invoices`]).
pub(crate) const INVOICE_LINES: LineTable = LineTable {
    table: "billing_invoice_lines",
    doc_column: "invoice_id",
};

// A quote's lines are deliberately NOT a `LineTable`. Alone among billing
// documents an offer may name the catalog item it sells (migration 0701), because
// accepting it can raise a sales order and an order line with no product is one
// nothing can ever be delivered against. That difference lives in
// [`crate::billing_quote_lines`], which reuses this module's line model and field
// rules and adds exactly one column — so a quote line and an invoice line are
// still the same thing where they are the same thing.

/// The template lines of a recurring arrangement
/// ([`crate::billing_schedules`], B2.11) — what next month's draft will say.
///
/// The same line model again, and for the sharpest version of the reason: what
/// a due run does is copy these onto an invoice, and a copy between two shapes
/// would be a place for a price or a rate to change on the way.
pub(crate) const SCHEDULE_LINES: LineTable = LineTable {
    table: "billing_schedule_lines",
    doc_column: "schedule_id",
};

/// The lines of a received bill ([`crate::billing_bills`]) — a supplier's
/// invoice line, read out of their e-invoice.
///
/// It is the same line model on purpose: their line and ours describe the same
/// thing (a quantity of something at a price at a rate), so the totals
/// arithmetic that checks their document is literally the one that computes
/// ours. Where the two differ is ownership, not shape, and that lives in
/// [`crate::billing_bills`].
pub(crate) const BILL_LINES: LineTable = LineTable {
    table: "billing_bill_lines",
    doc_column: "bill_id",
};

impl LineTable {
    /// The lines of one document of `tenant`, in print order.
    ///
    /// Takes any executor so the same read serves a plain pool read and a read
    /// inside the transaction that holds the document's lock.
    ///
    /// # Errors
    /// [`StoreError::Db`] on failure.
    pub(crate) async fn read<'e, E>(
        self,
        executor: E,
        tenant: &str,
        doc_id: &str,
    ) -> Result<Vec<Line>>
    where
        E: sqlx::Executor<'e, Database = sqlx::Postgres>,
    {
        let rows: Vec<LineRow> = sqlx::query_as(&format!(
            "SELECT {LINE_COLS} FROM {} WHERE tenant_id = $1 AND {} = $2 ORDER BY line_order",
            self.table, self.doc_column
        ))
        .bind(tenant)
        .bind(doc_id)
        .fetch_all(executor)
        .await
        .map_err(StoreError::Db)?;
        Ok(rows.into_iter().map(LineRow::into_line).collect())
    }

    /// Writes one line at `order`, with an id of its own.
    ///
    /// # Errors
    /// [`StoreError::Db`] on failure.
    pub(crate) async fn write(
        self,
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
        tenant: &str,
        doc_id: &str,
        order: i32,
        line: &NormalizedLine,
    ) -> Result<()> {
        sqlx::query(&format!(
            "INSERT INTO {} (tenant_id, {}, id, line_order, description, unit, qty_milli, \
                 unit_price_cents, vat_rate_bp) \
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)",
            self.table, self.doc_column
        ))
        .bind(tenant)
        .bind(doc_id)
        .bind(BillingLineId::generate().as_str())
        .bind(order)
        .bind(&line.description)
        .bind(&line.unit)
        .bind(line.qty_milli)
        .bind(line.unit_price_cents)
        .bind(line.vat_rate_bp)
        .execute(&mut **tx)
        .await
        .map_err(StoreError::Db)?;
        Ok(())
    }

    /// Replaces the whole line set of one document, in the caller's order,
    /// inside `tx`: either the document reads exactly as the caller sent it or
    /// it is untouched.
    ///
    /// Every line is validated **before** anything is written, so a document is
    /// never left half-replaced by a bad line at the end. The caller is
    /// expected to hold the document's row lock already — this function decides
    /// nothing about whether the document may be edited.
    ///
    /// # Errors
    /// [`StoreError::Validation`] when the set is too long or a line breaks a
    /// field rule (the message names the line's position);
    /// [`StoreError::Db`] on failure.
    pub(crate) async fn replace(
        self,
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
        tenant: &str,
        doc_id: &str,
        lines: &[NewLine],
    ) -> Result<()> {
        let lines: Vec<NormalizedLine> = normalize_lines(lines)?;

        sqlx::query(&format!(
            "DELETE FROM {} WHERE tenant_id = $1 AND {} = $2",
            self.table, self.doc_column
        ))
        .bind(tenant)
        .bind(doc_id)
        .execute(&mut **tx)
        .await
        .map_err(StoreError::Db)?;

        for (index, line) in lines.iter().enumerate() {
            // Unreachable while MAX_LINES is far below i32::MAX — kept because
            // a raised cap must fail loudly here, never wrap into a negative
            // print position.
            let order = i32::try_from(index)
                .map_err(|_| StoreError::Validation("a document has too many lines".to_owned()))?;
            self.write(tx, tenant, doc_id, order, line).await?;
        }
        Ok(())
    }
}

/// A stored line as read back, in [`LINE_COLS`] order.
#[derive(sqlx::FromRow)]
pub(crate) struct LineRow {
    id: String,
    pub(crate) line_order: i32,
    description: String,
    unit: String,
    qty_milli: i64,
    unit_price_cents: i64,
    vat_rate_bp: i32,
}

impl LineRow {
    /// The stored line.
    pub(crate) fn into_line(self) -> Line {
        Line {
            id: BillingLineId::new(self.id),
            line_order: self.line_order,
            description: self.description,
            unit: self.unit,
            qty_milli: self.qty_milli,
            unit_price_cents: self.unit_price_cents,
            vat_rate_bp: self.vat_rate_bp,
        }
    }
}

impl Line {
    /// This line as a writable line, unchanged — the copy an accepted quote
    /// puts on its invoice draft ([`crate::billing_quotes`]).
    ///
    /// It is already normalised: it was validated on the way in, and the rules
    /// are the same on both documents (that is the point of this module), so a
    /// copy can never fail a check the original passed. Copying the *frozen*
    /// values is the whole contract — a price that moved since the offer was
    /// made must not follow the customer onto the invoice.
    pub(crate) fn copied(&self) -> NormalizedLine {
        NormalizedLine {
            description: self.description.clone(),
            unit: self.unit.clone(),
            qty_milli: self.qty_milli,
            unit_price_cents: self.unit_price_cents,
            vat_rate_bp: self.vat_rate_bp,
        }
    }

    /// This line as a writable line with its quantity negated — the mirror a
    /// credit note is built from ([`crate::billing_invoices`]).
    ///
    /// It is already normalised: it was validated on the way in, and the
    /// quantity bound is symmetric ([`QTY_MAX_MILLI`]), so a stored quantity
    /// always has a storable negation.
    pub(crate) fn negated(&self) -> NormalizedLine {
        NormalizedLine {
            description: self.description.clone(),
            unit: self.unit.clone(),
            qty_milli: -self.qty_milli,
            unit_price_cents: self.unit_price_cents,
            vat_rate_bp: self.vat_rate_bp,
        }
    }
}

/// Just the numbers, for a list surface: the totals of many documents without
/// dragging every description over the wire. `doc_id` is whichever document
/// column the query aliased.
#[derive(sqlx::FromRow)]
pub(crate) struct FiguresRow {
    pub(crate) doc_id: String,
    pub(crate) qty_milli: i64,
    pub(crate) unit_price_cents: i64,
    pub(crate) vat_rate_bp: i32,
}

/// Groups the figures of many documents by document id, so a list read costs
/// one statement for the headers and one for every line of all of them —
/// never one per document.
pub(crate) fn group_figures(rows: Vec<FiguresRow>) -> HashMap<String, Vec<LineFigures>> {
    let mut by_doc: HashMap<String, Vec<LineFigures>> = HashMap::new();
    for row in rows {
        by_doc.entry(row.doc_id).or_default().push(LineFigures {
            qty_milli: row.qty_milli,
            unit_price_cents: row.unit_price_cents,
            vat_rate_bp: row.vat_rate_bp,
        });
    }
    by_doc
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::billing_field::{UNIT_PRICE_MAX_CENTS, VAT_RATE_MAX_BP};

    fn consulting() -> NewLine {
        NewLine {
            description: "Consulting".to_owned(),
            unit: "hour".to_owned(),
            qty_milli: 1_500,
            unit_price_cents: 12_000,
            vat_rate_bp: 2100,
        }
    }

    fn invalid<T: std::fmt::Debug>(result: Result<T>) -> String {
        match result {
            Err(StoreError::Validation(msg)) => msg,
            other => panic!("expected Validation, got {other:?}"),
        }
    }

    #[test]
    fn normalize_trims_and_keeps_the_numbers_exact() {
        let line = normalize_line(&NewLine {
            description: "  Consulting  ".to_owned(),
            unit: " hour ".to_owned(),
            ..consulting()
        })
        .unwrap_or_else(|e| panic!("rejected: {e}"));
        assert_eq!(line.description, "Consulting");
        assert_eq!(line.unit, "hour");
        assert_eq!(line.qty_milli, 1_500);
        assert_eq!(line.unit_price_cents, 12_000);
        assert_eq!(line.vat_rate_bp, 2100);
    }

    #[test]
    fn description_is_required_and_bounded() {
        for blank in ["", "   ", "\t\n"] {
            let bad = NewLine {
                description: blank.to_owned(),
                ..consulting()
            };
            assert!(invalid(normalize_line(&bad)).contains("description"));
        }
        let bad = NewLine {
            description: "x".repeat(LINE_DESCRIPTION_MAX_CHARS + 1),
            ..consulting()
        };
        assert!(invalid(normalize_line(&bad)).contains("at most"));
        // A description exactly as long as a product name still fits, so
        // picking any product from the price list can never truncate.
        let at_bound = NewLine {
            description: "x".repeat(crate::billing_products::PRODUCT_NAME_MAX_CHARS),
            ..consulting()
        };
        assert!(normalize_line(&at_bound).is_ok());
    }

    #[test]
    fn a_negative_quantity_is_a_discount_not_an_error() {
        let discount = NewLine {
            description: "Loyalty discount".to_owned(),
            qty_milli: -1_000,
            ..consulting()
        };
        assert_eq!(
            normalize_line(&discount)
                .unwrap_or_else(|e| panic!("rejected: {e}"))
                .qty_milli,
            -1_000
        );
        // Zero is fine too — a line that is there for its text.
        let zero = NewLine {
            qty_milli: 0,
            ..consulting()
        };
        assert!(normalize_line(&zero).is_ok());
    }

    #[test]
    fn quantity_is_capped_in_both_directions() {
        for ok in [0, 1, -1, QTY_MAX_MILLI, -QTY_MAX_MILLI] {
            let line = NewLine {
                qty_milli: ok,
                ..consulting()
            };
            assert!(normalize_line(&line).is_ok(), "expected valid: {ok}");
        }
        for bad in [QTY_MAX_MILLI + 1, -QTY_MAX_MILLI - 1, i64::MAX, i64::MIN] {
            let line = NewLine {
                qty_milli: bad,
                ..consulting()
            };
            assert!(
                invalid(normalize_line(&line)).contains("quantity"),
                "expected rejection: {bad}"
            );
        }
    }

    #[test]
    fn price_and_rate_follow_the_shared_billing_rules() {
        for bad in [
            NewLine {
                unit_price_cents: -1,
                ..consulting()
            },
            NewLine {
                unit_price_cents: UNIT_PRICE_MAX_CENTS + 1,
                ..consulting()
            },
            NewLine {
                vat_rate_bp: -1,
                ..consulting()
            },
            NewLine {
                vat_rate_bp: VAT_RATE_MAX_BP + 1,
                ..consulting()
            },
            NewLine {
                unit: "x".repeat(LINE_UNIT_MAX_CHARS + 1),
                ..consulting()
            },
        ] {
            assert!(matches!(
                normalize_line(&bad),
                Err(StoreError::Validation(_))
            ));
        }
    }

    #[test]
    fn a_rejected_line_is_named_by_its_position_but_never_quoted() {
        let lines = vec![
            consulting(),
            consulting(),
            NewLine {
                description: "Secret client project".to_owned(),
                qty_milli: i64::MAX,
                ..consulting()
            },
        ];
        let message = invalid(normalize_lines(&lines));
        assert!(message.contains("line 3"), "{message}");
        assert!(message.contains("quantity"), "{message}");
        assert!(
            !message.contains("Secret"),
            "a rule, never the customer's data: {message}"
        );
    }

    #[test]
    fn a_document_is_bounded_in_line_count() {
        let at_bound = vec![consulting(); MAX_LINES];
        assert_eq!(
            normalize_lines(&at_bound)
                .unwrap_or_else(|e| panic!("rejected: {e}"))
                .len(),
            MAX_LINES
        );
        let over = vec![consulting(); MAX_LINES + 1];
        assert!(invalid(normalize_lines(&over)).contains("at most"));
        // No lines at all is a legitimate draft.
        assert!(normalize_lines(&[]).is_ok());
    }

    #[test]
    fn a_stored_line_reports_its_own_net() {
        let line = Line {
            id: BillingLineId::generate(),
            line_order: 0,
            description: "Consulting".to_owned(),
            unit: "hour".to_owned(),
            qty_milli: 1_500,
            unit_price_cents: 12_000,
            vat_rate_bp: 2100,
        };
        // 1.5 h at €120.00 = €180.00; VAT is not a per-line number.
        assert_eq!(line.net_cents(), 18_000);
        assert_eq!(line.figures().vat_rate_bp, 2100);
    }

    #[test]
    fn negating_a_line_touches_only_its_quantity() {
        let line = Line {
            id: BillingLineId::generate(),
            line_order: 3,
            description: "Consulting".to_owned(),
            unit: "hour".to_owned(),
            qty_milli: 1_500,
            unit_price_cents: 12_000,
            vat_rate_bp: 2100,
        };
        let mirror = line.negated();
        assert_eq!(mirror.qty_milli, -1_500);
        assert_eq!(mirror.description, "Consulting");
        assert_eq!(mirror.unit, "hour");
        assert_eq!(mirror.unit_price_cents, 12_000, "a price is never negated");
        assert_eq!(mirror.vat_rate_bp, 2100);
        // A discount line credits back as a charge, and the extremes have a
        // storable mirror because the quantity bound is symmetric.
        assert_eq!(
            Line {
                qty_milli: -QTY_MAX_MILLI,
                ..line
            }
            .negated()
            .qty_milli,
            QTY_MAX_MILLI
        );
    }

    #[test]
    fn copying_a_line_changes_nothing_about_it() {
        // The copy an accepted quote makes onto its invoice draft: the same
        // words, the same frozen price and rate, the same quantity — a copy,
        // not a re-pricing.
        let line = Line {
            id: BillingLineId::generate(),
            line_order: 2,
            description: "Consulting".to_owned(),
            unit: "hour".to_owned(),
            qty_milli: -1_500,
            unit_price_cents: 12_000,
            vat_rate_bp: 2100,
        };
        let copy = line.copied();
        assert_eq!(copy.description, line.description);
        assert_eq!(copy.unit, line.unit);
        assert_eq!(copy.qty_milli, -1_500, "a discount line copies as one");
        assert_eq!(copy.unit_price_cents, 12_000);
        assert_eq!(copy.vat_rate_bp, 2100);
        assert_eq!(copy.figures().qty_milli, line.figures().qty_milli);
    }

    #[test]
    fn figures_group_by_their_document() {
        let row = |doc: &str, qty: i64| FiguresRow {
            doc_id: doc.to_owned(),
            qty_milli: qty,
            unit_price_cents: 1_000,
            vat_rate_bp: 2100,
        };
        let grouped = group_figures(vec![row("a", 1_000), row("b", 2_000), row("a", 3_000)]);
        assert_eq!(grouped.len(), 2);
        assert_eq!(
            grouped
                .get("a")
                .map(|lines| lines.iter().map(|line| line.qty_milli).sum::<i64>()),
            Some(4_000)
        );
        assert_eq!(grouped.get("b").map(Vec::len), Some(1));
        // A document with no lines is simply absent; the caller reads that as
        // an empty set rather than as a missing document.
        assert!(!grouped.contains_key("c"));
        assert!(group_figures(Vec::new()).is_empty());
    }
}
