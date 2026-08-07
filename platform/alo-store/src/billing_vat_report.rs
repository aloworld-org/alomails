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
//! **Currencies are never added together in their own group.** A document is
//! worth what it says in the currency it was raised in, so the summary is one
//! self-contained group per currency; a single-currency tenant — nearly all of
//! them — simply sees one.
//!
//! **And then they are added together once, in the accounting currency** (
//! B1.21), because a VAT return is filed in one currency and somebody has to be
//! able to copy a figure off this report. That total is built from each
//! document's **own frozen rate** ([`crate::billing_fx::FxSnapshot`]) — the rate
//! of its tax point, which is what EU VAT Directive art. 91 prescribes — never
//! from today's rate, so re-running last year's quarter answers last year's
//! figure. A document that carries no usable snapshot is **not** converted at a
//! guessed rate: it is counted as unconverted and reported as such, so a return
//! is never filed off a figure part of which was invented.
//!
//! Tenancy is structural: every statement carries `tenant_id` from the handle,
//! so another tenant's documents are not filtered out of the answer, they are
//! never read into it.

use time::Date;

use crate::account::AccountStore;
use crate::billing_fx::{FxSnapshot, convert_totals};
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
    /// What this group contributes to [`VatPeriod::base`]: the same documents
    /// restated in the accounting currency, each at its own frozen rate.
    ///
    /// A group whose documents all carry usable snapshots contributes all of
    /// itself; `unconverted_count` says how many did not and are therefore in
    /// none of the base figures.
    pub base_net_cents: i64,
    /// Sum of the documents' VAT in the accounting currency, in cents.
    pub base_vat_cents: i64,
    /// `base_net_cents + base_vat_cents`, in cents.
    pub base_gross_cents: i64,
    /// How many documents of this group could not be restated — no snapshot, or
    /// one taken against a different accounting currency than the tenant keeps
    /// books in today.
    pub unconverted_count: i64,
}

/// The whole period in the currency the tenant keeps books in: every document,
/// whatever it was raised in, at the rate frozen on it.
///
/// This is the figure a return is filed from; the per-currency groups above are
/// the paperwork it is justified by. The two can be checked against each other,
/// which is the point of reporting both.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct VatPeriodBase {
    /// ISO 4217 code the tenant keeps books in
    /// ([`crate::billing_settings::BillingSettings::base_currency`]).
    pub currency: String,
    /// Net across every currency, in cents of `currency`.
    pub net_cents: i64,
    /// VAT across every currency, in cents of `currency`.
    pub vat_cents: i64,
    /// `net_cents + vat_cents`, in cents of `currency`.
    pub gross_cents: i64,
    /// The breakdown by rate across every currency, ascending by rate — the
    /// per-rate boxes of a return.
    pub by_rate: Vec<VatPeriodRate>,
    /// How many documents in the period are in none of these figures because
    /// they could not be restated. **Non-zero means the total below is
    /// incomplete**, and the surface must say so rather than print it plain.
    pub unconverted_count: i64,
}

/// The VAT summary of one date range: the range it was asked for, a group per
/// currency ascending by code, and the whole period in the accounting currency.
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
    /// The same documents in the tenant's accounting currency. Its `currency`
    /// is stated even for an empty period: a report that says "nothing, in
    /// euro" is a fact, and one that omits the currency is a question.
    pub base: VatPeriodBase,
}

/// One document as the period sees it: what it is worth, in what currency,
/// whether it is a correction, and the rate it was frozen at.
///
/// The step between the two statements and the summary, so the aggregation
/// itself is pure and testable without a database.
#[derive(Debug, Clone)]
struct PeriodDocument {
    currency: String,
    is_credit_note: bool,
    totals: Totals,
    fx: Option<FxSnapshot>,
}

impl PeriodDocument {
    /// This document restated in `base`, or `None` when it cannot be — no
    /// snapshot, or one taken against a currency the tenant no longer keeps
    /// books in.
    ///
    /// The second case is deliberately not "convert anyway". A snapshot says
    /// "this document was restated into *that* currency at *this* rate"; reusing
    /// its number against a different base would be arithmetic on two unrelated
    /// facts, and the report says "unconverted" instead.
    fn restated_in(&self, base: &str) -> Option<Totals> {
        let fx = self.fx.as_ref()?;
        if fx.base_currency != base {
            return None;
        }
        convert_totals(&self.totals, fx.rate_micro)
    }
}

/// Adds the documents up into one group per currency, and into the one base
/// currency total a return is filed from.
///
/// Pure: no clock, no database, no tenant. Sums saturate rather than wrapping,
/// for the same reason [`crate::billing_totals`] does — a period holding an
/// absurd number of absurd documents gets an absurd figure, never a plausible
/// wrong one, and never a panic.
fn summarise(documents: &[PeriodDocument], base: &str) -> (Vec<VatPeriodCurrency>, VatPeriodBase) {
    // A tenant bills in a handful of currencies at most, and a document in a
    // handful of rates, so sorted vectors beat maps here and give the printed
    // order for free.
    let mut groups: Vec<VatPeriodCurrency> = Vec::new();
    let mut whole = VatPeriodBase {
        currency: base.to_owned(),
        ..VatPeriodBase::default()
    };
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
                        base_net_cents: 0,
                        base_vat_cents: 0,
                        base_gross_cents: 0,
                        unconverted_count: 0,
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
        add_rates(&mut group.by_rate, &document.totals);

        // The base side, document by document: each is crossed at its own frozen
        // rate and only then added, so the period's total is the sum of what the
        // documents themselves say — never the period's total crossed at one
        // rate, which would be a rate that applied to nothing.
        match document.restated_in(base) {
            Some(restated) => {
                group.base_net_cents = group.base_net_cents.saturating_add(restated.net_cents);
                group.base_vat_cents = group.base_vat_cents.saturating_add(restated.vat_cents);
                group.base_gross_cents =
                    group.base_gross_cents.saturating_add(restated.gross_cents);
                whole.net_cents = whole.net_cents.saturating_add(restated.net_cents);
                whole.vat_cents = whole.vat_cents.saturating_add(restated.vat_cents);
                whole.gross_cents = whole.gross_cents.saturating_add(restated.gross_cents);
                add_rates(&mut whole.by_rate, &restated);
            }
            None => {
                group.unconverted_count += 1;
                whole.unconverted_count += 1;
            }
        }
    }
    (groups, whole)
}

/// Adds a document's per-rate subtotals into a running breakdown, keeping it
/// ascending by rate and one row per rate.
fn add_rates(breakdown: &mut Vec<VatPeriodRate>, document: &Totals) {
    for subtotal in &document.vat_by_rate {
        match breakdown.binary_search_by_key(&subtotal.rate_bp, |r| r.rate_bp) {
            Ok(at) => {
                let row = &mut breakdown[at];
                row.net_cents = row.net_cents.saturating_add(subtotal.net_cents);
                row.vat_cents = row.vat_cents.saturating_add(subtotal.vat_cents);
            }
            Err(at) => breakdown.insert(
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

        type Header = (
            String,
            String,
            bool,
            Option<String>,
            Option<i64>,
            Option<Date>,
        );
        let headers: Vec<Header> = sqlx::query_as(&format!(
            "SELECT id, currency, is_credit_note, fx_base_currency, fx_rate_micro, fx_rate_date \
             FROM billing_invoices WHERE {IN_PERIOD}"
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
            .map(
                |(id, currency, is_credit_note, base_currency, rate_micro, rate_date)| {
                    PeriodDocument {
                        currency,
                        is_credit_note,
                        // A document with no lines is worth nothing and still
                        // counts as a document: it was issued, and it carries a
                        // number.
                        totals: totals(&by_document.remove(&id).unwrap_or_default()),
                        // All three columns or none: the table constrains them
                        // to move together, so a partial snapshot is not a state
                        // this can be read out of.
                        fx: base_currency.zip(rate_micro).zip(rate_date).map(
                            |((base_currency, rate_micro), rate_date)| FxSnapshot {
                                base_currency,
                                rate_micro,
                                rate_date,
                            },
                        ),
                    }
                },
            )
            .collect();

        // The accounting currency is read alongside the documents rather than
        // baked into them: it is what the report is being asked to express
        // itself in today, and a document whose snapshot names a different one
        // is reported as unconverted rather than re-crossed.
        let base = self.billing_base_currency().await?;
        let (currencies, whole) = summarise(&documents, &base);
        Ok(VatPeriod {
            from,
            to,
            currencies,
            base: whole,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::billing_fx::IDENTITY_RATE_MICRO;
    use crate::billing_totals::LineFigures;
    use time::Month;

    /// The day every snapshot below was published — a fixed past day, so no
    /// clock reaches into the arithmetic.
    fn published() -> Date {
        Date::from_calendar_date(2025, Month::August, 15).unwrap_or_else(|e| panic!("{e}"))
    }

    /// A document in the tenant's own accounting currency: the identity rate,
    /// which is what issuing a euro document in a euro-based tenant stamps.
    fn document(currency: &str, is_credit_note: bool, lines: &[LineFigures]) -> PeriodDocument {
        PeriodDocument {
            currency: currency.to_owned(),
            is_credit_note,
            totals: totals(lines),
            fx: Some(FxSnapshot::identity(currency, published())),
        }
    }

    /// A foreign-currency document with the rate it was issued at frozen on it:
    /// `rate_micro` units of `currency` per one euro.
    fn converted(
        currency: &str,
        is_credit_note: bool,
        lines: &[LineFigures],
        rate_micro: i64,
    ) -> PeriodDocument {
        PeriodDocument {
            currency: currency.to_owned(),
            is_credit_note,
            totals: totals(lines),
            fx: Some(FxSnapshot {
                base_currency: "EUR".to_owned(),
                rate_micro,
                rate_date: published(),
            }),
        }
    }

    /// A document carrying no snapshot at all — one issued before B1.21 in a
    /// currency that was not the tenant's own.
    fn unconverted(currency: &str, lines: &[LineFigures]) -> PeriodDocument {
        PeriodDocument {
            currency: currency.to_owned(),
            is_credit_note: false,
            totals: totals(lines),
            fx: None,
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

    /// The report of these documents for a euro-based tenant.
    fn in_euro(documents: &[PeriodDocument]) -> (Vec<VatPeriodCurrency>, VatPeriodBase) {
        summarise(documents, "EUR")
    }

    fn only(groups: &[VatPeriodCurrency]) -> &VatPeriodCurrency {
        assert_eq!(groups.len(), 1, "expected one currency: {groups:?}");
        groups.first().unwrap_or_else(|| unreachable!())
    }

    #[test]
    fn a_period_with_no_documents_summarises_to_nothing() {
        let (groups, base) = in_euro(&[]);
        assert!(groups.is_empty(), "no currencies, not a zero row");
        // The base side still says which currency it is nothing in: a figure of
        // zero under a stated currency is an answer, one without is a question.
        assert_eq!(base.currency, "EUR");
        assert_eq!(base.net_cents, 0);
        assert_eq!(base.unconverted_count, 0);
        assert!(base.by_rate.is_empty());
    }

    #[test]
    fn documents_add_up_per_rate_and_the_totals_are_the_sum_of_the_rows() {
        // Two invoices: 10 × €100 at 21 % (net 100 000, VAT 21 000) and
        // 1 × €500 at 21 % plus 1 × €250 at 9 % (net 50 000 + 25 000,
        // VAT 10 500 + 2 250). Hand-computed, outside the code under test.
        let (groups, base) = in_euro(&[
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
        // A single-currency tenant's base side is its own figures, unmoved: the
        // conversion is the identity, so the two tables agree exactly.
        assert_eq!(base.net_cents, eur.net_cents);
        assert_eq!(base.vat_cents, eur.vat_cents);
        assert_eq!(base.gross_cents, eur.gross_cents);
        assert_eq!(base.by_rate, eur.by_rate);
        assert_eq!(base.unconverted_count, 0);
        assert_eq!(eur.base_net_cents, eur.net_cents);
        assert_eq!(eur.base_vat_cents, eur.vat_cents);
        assert_eq!(eur.base_gross_cents, eur.gross_cents);
    }

    #[test]
    fn a_credit_note_subtracts_and_is_counted_apart() {
        let invoice = line(10, 10_000, 2100);
        let credited = LineFigures {
            qty_milli: -invoice.qty_milli,
            ..invoice
        };
        let (groups, _) = in_euro(&[
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
    fn a_credit_note_at_its_originals_rate_nets_to_zero_in_the_base_currency_too() {
        // Why a credit note inherits its original's rate rather than taking the
        // rate of the day it was raised: at one rate the pair cancels exactly,
        // at two it leaves a residue in the books that nothing on either
        // document explains.
        let invoice = line(10, 10_000, 2100);
        let credited = LineFigures {
            qty_milli: -invoice.qty_milli,
            ..invoice
        };
        let (groups, base) = in_euro(&[
            converted("USD", false, &[invoice], 1_162_600),
            converted("USD", true, &[credited], 1_162_600),
        ]);
        let usd = only(&groups);
        assert_eq!(usd.base_net_cents, 0);
        assert_eq!(usd.base_vat_cents, 0);
        assert_eq!(usd.base_gross_cents, 0);
        assert_eq!(base.net_cents, 0);
        assert_eq!(base.vat_cents, 0);
        assert_eq!(base.unconverted_count, 0);
    }

    #[test]
    fn the_period_sums_what_the_documents_charged_never_the_rate_re_applied() {
        // Three separate documents of 9.99 at 21 %: each charges 2.10 (0.21 ×
        // 9.99 = 2.0979 → 2.10), so the period's tax is 6.30. Applying 21 % to
        // the summed net of 29.97 would give 6.2937 → 6.29, a cent less than
        // the customers were actually charged.
        let (groups, _) = in_euro(&[
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
        let (groups, _) = in_euro(&[
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
    fn every_currency_reaches_the_base_total_at_its_own_documents_rate() {
        // A euro document, a dollar one at 1 EUR = 1.1626 USD and a yen one at
        // 1 EUR = 171.42 JPY. Each is crossed on its own and only then added:
        //   USD net 50 000 / 1.1626 = 43 007.05… → 43 007 ; VAT 10 500 → 9 031.48… → 9 031
        //   JPY net 1 714 200 / 171.42 = 10 000   → 10 000 ; VAT      0 →           0
        let (groups, base) = in_euro(&[
            document("EUR", false, &[line(1, 10_000, 2100)]),
            converted("USD", false, &[line(1, 50_000, 2100)], 1_162_600),
            converted("JPY", false, &[line(1, 1_714_200, 0)], 171_420_000),
        ]);
        assert_eq!(groups.len(), 3);
        assert_eq!(groups[2].currency, "USD");
        assert_eq!(groups[2].base_net_cents, 43_007);
        assert_eq!(groups[2].base_vat_cents, 9_031);
        assert_eq!(groups[2].base_gross_cents, 52_038);
        assert_eq!(groups[1].currency, "JPY");
        assert_eq!(groups[1].base_net_cents, 10_000);
        // The base total is the sum of the three, per rate — the figures a
        // return is filed from.
        assert_eq!(base.currency, "EUR");
        assert_eq!(base.net_cents, 10_000 + 43_007 + 10_000);
        assert_eq!(base.vat_cents, 2_100 + 9_031);
        assert_eq!(base.gross_cents, base.net_cents + base.vat_cents);
        assert_eq!(
            base.by_rate,
            vec![
                VatPeriodRate {
                    rate_bp: 0,
                    net_cents: 10_000,
                    vat_cents: 0,
                },
                VatPeriodRate {
                    rate_bp: 2100,
                    net_cents: 10_000 + 43_007,
                    vat_cents: 2_100 + 9_031,
                },
            ]
        );
        assert_eq!(
            base.net_cents,
            base.by_rate.iter().map(|r| r.net_cents).sum::<i64>(),
            "the base rows add up to the base total"
        );
    }

    #[test]
    fn a_document_without_a_usable_snapshot_is_counted_apart_never_guessed_at() {
        let (groups, base) = in_euro(&[
            document("EUR", false, &[line(1, 10_000, 2100)]),
            // No snapshot at all (issued before B1.21).
            unconverted("USD", &[line(1, 50_000, 2100)]),
            // A snapshot into a currency the tenant no longer keeps books in:
            // its rate says nothing about euro, so it is not applied to euro.
            PeriodDocument {
                fx: Some(FxSnapshot {
                    base_currency: "CHF".to_owned(),
                    rate_micro: 1_162_600,
                    rate_date: published(),
                }),
                ..unconverted("USD", &[line(1, 20_000, 2100)])
            },
        ]);
        let usd = &groups[1];
        assert_eq!(usd.currency, "USD");
        assert_eq!(usd.invoice_count, 2, "both stand and both are reported");
        assert_eq!(usd.net_cents, 70_000, "in dollars, they add up as ever");
        assert_eq!(usd.unconverted_count, 2);
        assert_eq!(
            usd.base_net_cents, 0,
            "no rate, no figure — not a zero rate"
        );
        assert_eq!(base.unconverted_count, 2);
        assert_eq!(
            base.net_cents, 10_000,
            "the base total holds only what could be restated, and says how much could not"
        );
    }

    #[test]
    fn a_document_with_no_lines_counts_without_moving_a_figure() {
        let (groups, base) = in_euro(&[
            document("EUR", false, &[line(10, 10_000, 2100)]),
            document("EUR", false, &[]),
        ]);
        let eur = only(&groups);
        assert_eq!(eur.invoice_count, 2, "it was issued; it counts");
        assert_eq!(eur.net_cents, 100_000);
        assert_eq!(eur.by_rate.len(), 1, "it used no rate, so it added no row");
        // It converts to nothing, which is not the same as failing to convert.
        assert_eq!(base.unconverted_count, 0);
        assert_eq!(base.net_cents, 100_000);
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
        let (groups, base) = in_euro(&documents);
        let eur = only(&groups);
        assert_eq!(eur.net_cents, i64::MAX, "saturated, not wrapped negative");
        assert_eq!(eur.gross_cents, i64::MAX);
        assert_eq!(eur.by_rate.len(), 1);
        assert_eq!(base.net_cents, i64::MAX);
        assert_eq!(base.gross_cents, i64::MAX);
    }

    #[test]
    fn a_documents_own_currency_is_what_decides_whether_it_is_restated() {
        // The identity snapshot of a euro document in a euro-based tenant is
        // applied (it is the same figure), and the same document in a
        // franc-based tenant is not: its snapshot names euro, and euro is not
        // what those books are kept in.
        let euro_document = document("EUR", false, &[line(1, 10_000, 2100)]);
        let (_, in_euros) = summarise(std::slice::from_ref(&euro_document), "EUR");
        assert_eq!(in_euros.net_cents, 10_000);
        assert_eq!(in_euros.unconverted_count, 0);
        let (groups, in_francs) = summarise(std::slice::from_ref(&euro_document), "CHF");
        assert_eq!(in_francs.currency, "CHF");
        assert_eq!(in_francs.net_cents, 0);
        assert_eq!(in_francs.unconverted_count, 1);
        assert_eq!(
            only(&groups).net_cents,
            10_000,
            "the document itself is reported exactly as it was raised"
        );
        assert_eq!(
            FxSnapshot::identity("EUR", published()).rate_micro,
            IDENTITY_RATE_MICRO
        );
    }
}
