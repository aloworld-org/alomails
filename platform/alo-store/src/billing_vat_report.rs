//! The VAT summary of a period (alo Billing, ADR 0035, wave B1.20), reached
//! through the account door like [`crate::billing_invoices`].
//!
//! This is the figure a bookkeeper copies onto a VAT return: for one date
//! range, how much was billed at each rate and how much tax that came to. It is
//! a **read**, computed from the documents themselves every time — nothing here
//! is stored, so a return can never quote a subtotal that the invoices behind
//! it no longer add up to.
//!
//! Four decisions make it the figure a tax authority would recognise, and each
//! of them is the strict reading rather than the convenient one:
//!
//! - **The period is judged on the issue date**, the date frozen on the
//!   document when it was numbered — not the day it was keyed in, and not the
//!   day the money arrived. Under the ordinary invoice-based (accrual) VAT
//!   scheme that is the tax point, and it is the only date on the document that
//!   cannot be moved afterwards. A cash-accounting variant would be a different
//!   report over the payments and is deliberately not this one
//!   (`docs/design/billing.md`).
//! - **Only documents that stand are counted**: `issued` and `paid`. A `draft`
//!   was never raised and carries no number; a `void` one was cancelled and its
//!   number is kept only so the series stays gapless. Neither charged anybody
//!   any tax.
//! - **Credit notes are included, negatively.** They already carry negated
//!   lines, so they subtract by construction, which is exactly what a correction
//!   does to a period's output tax. They are counted separately as well, because
//!   a period whose net is small because it was quiet and one whose net is small
//!   because half of it was credited are different facts.
//! - **Each document's own rounded VAT is summed** — never the rate re-applied
//!   to the summed net. The tax charged in a period is the sum of the tax on the
//!   documents the customers hold; recomputing it from the total net would
//!   differ by cents from those documents, and a return that disagrees with the
//!   invoices behind it is the defect this rule exists to prevent
//!   ([`crate::billing_totals`] rounds once per rate subtotal, per document).
//!
//! **Currencies are never added together.** A document is worth what it says in
//! the currency it was raised in, and until the rate snapshots of B1.21 exist
//! there is no honest way to express a dollar invoice in euro. The summary is
//! therefore one group per currency, each self-contained; a single-currency
//! tenant — nearly all of them — simply sees one.
//!
//! Tenancy is structural: both statements carry `tenant_id` from the handle, so
//! another tenant's documents are not filtered out of the answer, they are never
//! read into it.

use time::Date;

use crate::account::AccountStore;
use crate::billing_line::{FiguresRow, group_figures};
use crate::billing_totals::{Totals, totals};
use crate::error::{Result, StoreError};

/// What was billed at one VAT rate in the period, and the tax on it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VatPeriodRate {
    /// The rate, in basis points (2100 = 21 %).
    pub rate_bp: i32,
    /// The sum of the documents' net at this rate, in cents.
    pub net_cents: i64,
    /// The sum of the documents' own VAT at this rate, in cents.
    pub vat_cents: i64,
}

/// The period's figures in one currency: the breakdown by rate, what it adds
/// up to, and how many documents it was taken from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VatPeriodCurrency {
    /// ISO 4217 code the documents in this group were raised in.
    pub currency: String,
    /// How many ordinary invoices contributed.
    pub invoice_count: i64,
    /// How many credit notes contributed — the corrections, which subtract.
    pub credit_note_count: i64,
    /// Sum of the documents' net, in cents.
    pub net_cents: i64,
    /// Sum of the documents' VAT, in cents.
    pub vat_cents: i64,
    /// `net_cents + vat_cents`, in cents.
    pub gross_cents: i64,
    /// The breakdown, ascending by rate, one row per rate that appears.
    pub by_rate: Vec<VatPeriodRate>,
}

/// The VAT summary of one date range: the range it was asked for, and a group
/// per currency, ascending by code.
///
/// The range is echoed back because the report is a document a human puts under
/// a return: it has to say which days it covers, in the store's own words
/// rather than in whatever the caller thinks it asked for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VatPeriod {
    /// First day included.
    pub from: Date,
    /// Last day included — inclusive, like every date range a bookkeeper
    /// writes ("1 July to 30 September").
    pub to: Date,
    /// One group per currency present, ascending by code. Empty when the
    /// period holds no documents at all.
    pub currencies: Vec<VatPeriodCurrency>,
}

/// One document as the period sees it: what it is worth, in what currency, and
/// whether it is a correction.
///
/// The step between the two statements and the summary, so the aggregation
/// itself is pure and testable without a database.
#[derive(Debug, Clone)]
struct PeriodDocument {
    currency: String,
    is_credit_note: bool,
    totals: Totals,
}

/// Adds the documents up into one group per currency.
///
/// Pure: no clock, no database, no tenant. Sums saturate rather than wrapping,
/// for the same reason [`crate::billing_totals`] does — a period holding an
/// absurd number of absurd documents gets an absurd figure, never a plausible
/// wrong one, and never a panic.
fn summarise(documents: &[PeriodDocument]) -> Vec<VatPeriodCurrency> {
    // A tenant bills in a handful of currencies at most, and a document in a
    // handful of rates, so sorted vectors beat maps here and give the printed
    // order for free.
    let mut groups: Vec<VatPeriodCurrency> = Vec::new();
    for document in documents {
        let at = match groups.binary_search_by(|g| g.currency.as_str().cmp(&document.currency)) {
            Ok(at) => at,
            Err(at) => {
                groups.insert(
                    at,
                    VatPeriodCurrency {
                        currency: document.currency.clone(),
                        invoice_count: 0,
                        credit_note_count: 0,
                        net_cents: 0,
                        vat_cents: 0,
                        gross_cents: 0,
                        by_rate: Vec::new(),
                    },
                );
                at
            }
        };
        let group = &mut groups[at];
        if document.is_credit_note {
            group.credit_note_count += 1;
        } else {
            group.invoice_count += 1;
        }
        group.net_cents = group.net_cents.saturating_add(document.totals.net_cents);
        group.vat_cents = group.vat_cents.saturating_add(document.totals.vat_cents);
        group.gross_cents = group
            .gross_cents
            .saturating_add(document.totals.gross_cents);
        for subtotal in &document.totals.vat_by_rate {
            match group
                .by_rate
                .binary_search_by_key(&subtotal.rate_bp, |r| r.rate_bp)
            {
                Ok(at) => {
                    let row = &mut group.by_rate[at];
                    row.net_cents = row.net_cents.saturating_add(subtotal.net_cents);
                    row.vat_cents = row.vat_cents.saturating_add(subtotal.vat_cents);
                }
                Err(at) => group.by_rate.insert(
                    at,
                    VatPeriodRate {
                        rate_bp: subtotal.rate_bp,
                        net_cents: subtotal.net_cents,
                        vat_cents: subtotal.vat_cents,
                    },
                ),
            }
        }
    }
    groups
}

impl AccountStore {
    /// The VAT summary of this tenant's documents issued between `from` and
    /// `to`, both days included.
    ///
    /// Two statements whatever the length of the period: the documents that
    /// stand in it, then every line of all of them. Each document's totals are
    /// then computed by [`crate::billing_totals`] — the same code the document
    /// itself, its PDF and its e-invoice are printed from — and summed per
    /// currency and per rate, so the report and the paperwork can never
    /// disagree about a cent.
    ///
    /// # Errors
    /// [`StoreError::Validation`] when the period ends before it starts;
    /// [`StoreError::Db`] on failure.
    pub async fn billing_vat_period(&self, from: Date, to: Date) -> Result<VatPeriod> {
        if to < from {
            return Err(StoreError::Validation(
                "the end of the period must not be before its start".to_owned(),
            ));
        }

        // The one predicate both reads state, so the lines fetched are exactly
        // the lines of the documents counted — no window in which a second
        // spelling of "in the period" could drift from the first.
        const IN_PERIOD: &str = "tenant_id = $1 AND status IN ('issued', 'paid') \
             AND issue_date >= $2 AND issue_date <= $3";

        let headers: Vec<(String, String, bool)> = sqlx::query_as(&format!(
            "SELECT id, currency, is_credit_note FROM billing_invoices WHERE {IN_PERIOD}"
        ))
        .bind(self.tenant.as_str())
        .bind(from)
        .bind(to)
        .fetch_all(&self.pool)
        .await
        .map_err(StoreError::Db)?;

        let figures = sqlx::query_as::<_, FiguresRow>(&format!(
            "SELECT invoice_id AS doc_id, qty_milli, unit_price_cents, vat_rate_bp \
             FROM billing_invoice_lines \
             WHERE tenant_id = $1 AND invoice_id IN ( \
                 SELECT id FROM billing_invoices WHERE {IN_PERIOD})"
        ))
        .bind(self.tenant.as_str())
        .bind(from)
        .bind(to)
        .fetch_all(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        let mut by_document = group_figures(figures);

        let documents: Vec<PeriodDocument> = headers
            .into_iter()
            .map(|(id, currency, is_credit_note)| PeriodDocument {
                currency,
                is_credit_note,
                // A document with no lines is worth nothing and still counts as
                // a document: it was issued, and it carries a number.
                totals: totals(&by_document.remove(&id).unwrap_or_default()),
            })
            .collect();

        Ok(VatPeriod {
            from,
            to,
            currencies: summarise(&documents),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::billing_totals::LineFigures;

    /// A document of the given lines, in a currency, as an invoice or as the
    /// credit note of one (whose lines are already negated by the store, so a
    /// test writes them negated too).
    fn document(currency: &str, is_credit_note: bool, lines: &[LineFigures]) -> PeriodDocument {
        PeriodDocument {
            currency: currency.to_owned(),
            is_credit_note,
            totals: totals(lines),
        }
    }

    /// `units` whole units at `price_cents`, taxed at `rate_bp`.
    fn line(units: i64, price_cents: i64, rate_bp: i32) -> LineFigures {
        LineFigures {
            qty_milli: units * 1_000,
            unit_price_cents: price_cents,
            vat_rate_bp: rate_bp,
        }
    }

    fn only(groups: &[VatPeriodCurrency]) -> &VatPeriodCurrency {
        assert_eq!(groups.len(), 1, "expected one currency: {groups:?}");
        groups.first().unwrap_or_else(|| unreachable!())
    }

    #[test]
    fn a_period_with_no_documents_summarises_to_nothing() {
        assert!(summarise(&[]).is_empty(), "no currencies, not a zero row");
    }

    #[test]
    fn documents_add_up_per_rate_and_the_totals_are_the_sum_of_the_rows() {
        // Two invoices: 10 × €100 at 21 % (net 100 000, VAT 21 000) and
        // 1 × €500 at 21 % plus 1 × €250 at 9 % (net 50 000 + 25 000,
        // VAT 10 500 + 2 250). Hand-computed, outside the code under test.
        let groups = summarise(&[
            document("EUR", false, &[line(10, 10_000, 2100)]),
            document("EUR", false, &[line(1, 50_000, 2100), line(1, 25_000, 900)]),
        ]);
        let eur = only(&groups);
        assert_eq!(eur.invoice_count, 2);
        assert_eq!(eur.credit_note_count, 0);
        assert_eq!(
            eur.by_rate,
            vec![
                VatPeriodRate {
                    rate_bp: 900,
                    net_cents: 25_000,
                    vat_cents: 2_250,
                },
                VatPeriodRate {
                    rate_bp: 2100,
                    net_cents: 150_000,
                    vat_cents: 31_500,
                },
            ],
            "one row per rate, ascending, both invoices' 21 % in one row"
        );
        assert_eq!(eur.net_cents, 175_000);
        assert_eq!(eur.vat_cents, 33_750);
        assert_eq!(eur.gross_cents, 208_750);
        // The totals are exactly the rows, which is what makes the report
        // checkable by hand.
        assert_eq!(
            eur.net_cents,
            eur.by_rate.iter().map(|r| r.net_cents).sum::<i64>()
        );
        assert_eq!(
            eur.vat_cents,
            eur.by_rate.iter().map(|r| r.vat_cents).sum::<i64>()
        );
    }

    #[test]
    fn a_credit_note_subtracts_and_is_counted_apart() {
        let invoice = line(10, 10_000, 2100);
        let credited = LineFigures {
            qty_milli: -invoice.qty_milli,
            ..invoice
        };
        let groups = summarise(&[
            document("EUR", false, &[invoice]),
            document("EUR", true, &[credited]),
        ]);
        let eur = only(&groups);
        assert_eq!(eur.invoice_count, 1);
        assert_eq!(eur.credit_note_count, 1, "counted apart, not as an invoice");
        // A full credit takes the period back to zero, to the cent — the
        // rounding convention is chosen so that holds.
        assert_eq!(eur.net_cents, 0);
        assert_eq!(eur.vat_cents, 0);
        assert_eq!(eur.gross_cents, 0);
        assert_eq!(
            eur.by_rate,
            vec![VatPeriodRate {
                rate_bp: 2100,
                net_cents: 0,
                vat_cents: 0,
            }],
            "the rate is still reported: it was used, and it netted out"
        );
    }

    #[test]
    fn the_period_sums_what_the_documents_charged_never_the_rate_re_applied() {
        // Three separate documents of 9.99 at 21 %: each charges 2.10 (0.21 ×
        // 9.99 = 2.0979 → 2.10), so the period's tax is 6.30. Applying 21 % to
        // the summed net of 29.97 would give 6.2937 → 6.29, a cent less than
        // the customers were actually charged.
        let groups = summarise(&[
            document("EUR", false, &[line(3, 333, 2100)]),
            document("EUR", false, &[line(3, 333, 2100)]),
            document("EUR", false, &[line(3, 333, 2100)]),
        ]);
        let eur = only(&groups);
        assert_eq!(eur.net_cents, 2_997);
        assert_eq!(eur.vat_cents, 630, "3 × 210, not 21 % of 2997");
        assert_eq!(eur.gross_cents, 3_627);
    }

    #[test]
    fn currencies_are_grouped_and_never_added_together() {
        let groups = summarise(&[
            document("USD", false, &[line(1, 10_000, 0)]),
            document("EUR", false, &[line(1, 10_000, 2100)]),
            document("USD", true, &[line(-1, 5_000, 0)]),
        ]);
        assert_eq!(
            groups
                .iter()
                .map(|g| g.currency.as_str())
                .collect::<Vec<_>>(),
            vec!["EUR", "USD"],
            "one group per currency, ascending by code"
        );
        assert_eq!(groups[0].net_cents, 10_000);
        assert_eq!(groups[0].vat_cents, 2_100);
        assert_eq!(groups[1].net_cents, 5_000, "the dollar group nets its own");
        assert_eq!(groups[1].vat_cents, 0);
        assert_eq!(groups[1].invoice_count, 1);
        assert_eq!(groups[1].credit_note_count, 1);
    }

    #[test]
    fn a_document_with_no_lines_counts_without_moving_a_figure() {
        let groups = summarise(&[
            document("EUR", false, &[line(10, 10_000, 2100)]),
            document("EUR", false, &[]),
        ]);
        let eur = only(&groups);
        assert_eq!(eur.invoice_count, 2, "it was issued; it counts");
        assert_eq!(eur.net_cents, 100_000);
        assert_eq!(eur.by_rate.len(), 1, "it used no rate, so it added no row");
    }

    #[test]
    fn an_absurd_period_saturates_rather_than_wrapping() {
        // Nothing the store accepts can reach here: it would take more
        // documents than a tenant can raise. The guarantee is that the sum is
        // total for any input, including a future caller's.
        let biggest = vec![
            LineFigures {
                qty_milli: 1_000_000_000,
                unit_price_cents: 1_000_000_000,
                vat_rate_bp: 10_000,
            };
            500
        ];
        let documents: Vec<PeriodDocument> =
            (0..64).map(|_| document("EUR", false, &biggest)).collect();
        let groups = summarise(&documents);
        let eur = only(&groups);
        assert_eq!(eur.net_cents, i64::MAX, "saturated, not wrapped negative");
        assert_eq!(eur.gross_cents, i64::MAX);
        assert_eq!(eur.by_rate.len(), 1);
    }
}
