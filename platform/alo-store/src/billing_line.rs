//! The line of a billing document (alo Billing, ADR 0035, wave B1) — shared
//! by invoices and, from B1.11, by quotes.
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
}
