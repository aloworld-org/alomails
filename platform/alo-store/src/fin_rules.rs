//! The posting rules — what a document does to the books (ADR 0035, wave
//! B4.04; `docs/design/finance.md`, "Posting rules, per document type").
//!
//! Every rule here is a **pure function from a document to a
//! [`NewEntry`]**: no database, no clock, no tenant lookup beyond the accounts
//! the caller already resolved by role. That is what lets each one be read
//! against a hand-written golden entry — the debits and credits an accountant
//! would write out on paper — before it is ever wired into a transaction, and
//! it is why this file contains no `async` at all.
//!
//! Three rules of the note that this file makes executable:
//!
//! - **The ledger books what billing computed.** Every figure comes from
//!   [`crate::billing_totals::Totals`] as the document itself printed it —
//!   the gross, the per-rate net, the per-rate tax. Nothing is recomputed
//!   here, so the invoice, the PDF, the e-invoice XML and the journal cannot
//!   disagree about a cent.
//! - **An account is found by role, never by code** — the caller hands the
//!   resolved accounts in ([`InvoiceAccounts`]), and a chart that is missing
//!   one refuses the *document* rather than posting to a suspense account
//!   (that is [`crate::AccountStore::post_invoice_issue`]'s job, one layer up).
//! - **`entry_date` is the document's date, not today.** An invoice books on
//!   its issue date, which is what makes a period report and the soft close
//!   (B4.10) mean anything.
//!
//! ## The base column, and why this rule can never leave a residual
//!
//! Each posting carries the document's own currency and the same money in the
//! tenant's accounting currency, crossed at the rate frozen on the document
//! (B1.21). The note's doctrine is `billing_fx::convert_totals`': **cross the
//! parts, sum the parts, never cross the whole.** Here the parts are the
//! revenue and tax postings, and the whole is the receivable — so the
//! receivable's base amount is *defined* as the sum of the crossed parts
//! rather than as the crossed gross.
//!
//! Two things follow, both wanted. The entry balances in the base column by
//! construction, so the invoice rule never needs the `rounding` account (the
//! rules whose postings are each independently crossed — a settlement, B4.04b —
//! are where that account earns its keep). And the receivable the books carry
//! is **exactly** the figure [`crate::billing_fx::restated_into`] reports for
//! the same document, which is the figure the document prints and the VAT
//! report sums; a ledger that disagreed with the paper by a cent per invoice
//! would be discovered a year later by somebody reconciling both.

use crate::billing_fx::{FxSnapshot, convert_cents};
use crate::billing_invoices::{InvoiceDocument, InvoiceStatus};
use crate::error::{Result, StoreError};
use crate::fin_journal::{EntryKind, EntrySource, NewEntry, NewPosting, SourceEvent, SourceKind};
use crate::id::FinAccountId;

/// The accounts an invoice's rule needs, resolved by role before it is called.
///
/// A struct rather than three arguments so a caller cannot silently swap two
/// of them — booking revenue to the receivable balances just as well and is
/// wrong in a way no invariant would catch.
#[derive(Debug, Clone)]
pub struct InvoiceAccounts {
    /// Trade receivables — the debit: what the customer now owes.
    pub ar: FinAccountId,
    /// Sales revenue — where each rate's net lands.
    pub revenue: FinAccountId,
    /// VAT we charged and owe the state, per rate.
    pub vat_output: FinAccountId,
}

/// The entry an **issued invoice** books: the receivable against the revenue
/// and the output tax it is made of.
///
/// ```text
/// debit   ar        gross         dimension: customer
/// credit  revenue   net per rate  dimension: vat rate
/// credit  vat_output tax per rate dimension: vat rate
/// ```
///
/// **Revenue is one posting per VAT rate, not one per line.** The note's table
/// says "per line, dimension `project_id` when the line came from B3", and a
/// billing line does not carry a project today (`billing_line::Line` holds
/// description, quantity, price and rate — the B3 handoff writes the hours into
/// the *description*). One posting per line would therefore multiply a
/// 400-line invoice into 400 identical-looking credits carrying no information
/// the rate grouping does not already carry. When a line gains a project link,
/// this rule splits the per-rate credit by project and nothing else about it
/// changes — the assertions below are written per rate, which a split by
/// project still satisfies in aggregate.
///
/// **The rate travels as a dimension on the revenue posting too**, not only on
/// the tax posting. A VAT return needs the taxable base per rate as well as the
/// tax per rate, and taking both from the journal — rather than the tax from
/// the journal and the base from the documents — is what makes the return and
/// the books provably the same statement.
///
/// A posting whose two columns are both zero is dropped rather than written: a
/// rate group that nets to nothing (a line and its exact discount) is not an
/// event, and the journal refuses a posting that moves no money.
///
/// # Errors
/// [`StoreError::Conflict`] when the document is not one that books at issue —
/// a draft is an intention, a void one is booked by its reversal, and a credit
/// note has its own rule (B4.04c). [`StoreError::Validation`] when the
/// document cannot be expressed in the books: no issue date, no exchange-rate
/// snapshot for a foreign-currency document, a snapshot taken against a
/// different accounting currency, an amount that cannot be crossed, or a
/// document that books nothing at all.
pub fn invoice_issue_entry(
    document: &InvoiceDocument,
    base_currency: &str,
    accounts: &InvoiceAccounts,
) -> Result<NewEntry> {
    let invoice = &document.invoice;
    if invoice.is_credit_note {
        return Err(StoreError::Conflict(
            "a credit note is booked by the credit-note rule, not as an invoice".to_owned(),
        ));
    }
    match invoice.status {
        // `paid` is bookable: it was issued first, and a backfill meets
        // documents that have since been settled.
        InvoiceStatus::Issued | InvoiceStatus::Paid => {}
        InvoiceStatus::Draft => {
            return Err(StoreError::Conflict(
                "a draft invoice is an intention, not an event; issue it before booking it"
                    .to_owned(),
            ));
        }
        InvoiceStatus::Void => {
            return Err(StoreError::Conflict(
                "a void invoice is booked by its issue entry and reversed by its void entry"
                    .to_owned(),
            ));
        }
    }
    let entry_date = invoice.issue_date.ok_or_else(|| {
        StoreError::Validation(
            "an issued invoice must carry an issue date before it can be booked".to_owned(),
        )
    })?;

    let fx = booking_rate(document, base_currency, entry_date)?;
    let cross = |cents: i64| {
        convert_cents(cents, fx.rate_micro).ok_or_else(|| {
            StoreError::Validation(
                "the invoice's exchange rate cannot restate it into the accounting currency"
                    .to_owned(),
            )
        })
    };

    // The credits first, so the receivable can be the sum of them in both
    // columns — the module header's whole argument about the base column.
    let mut postings: Vec<NewPosting> = Vec::with_capacity(document.totals.vat_by_rate.len() * 2);
    let mut credit_cents: i64 = 0;
    let mut credit_base_cents: i64 = 0;
    for subtotal in &document.totals.vat_by_rate {
        for (account, cents) in [
            (&accounts.revenue, subtotal.net_cents),
            (&accounts.vat_output, subtotal.vat_cents),
        ] {
            let base = cross(cents)?;
            if cents == 0 && base == 0 {
                continue;
            }
            credit_cents = add(credit_cents, cents)?;
            credit_base_cents = add(credit_base_cents, base)?;
            postings.push(NewPosting {
                vat_rate_bp: Some(subtotal.rate_bp),
                // Credits are negative: the sign is the direction
                // (`docs/design/finance.md`, "Signed amounts").
                ..NewPosting::new(account.clone(), -cents, -base)
            });
        }
    }

    // The receivable. Its document amount is the gross billing computed — P3 in
    // one line — and its base amount is what the credits actually crossed to.
    debug_assert_eq!(
        credit_cents, document.totals.gross_cents,
        "the per-rate rows are exactly the document's gross"
    );
    if credit_cents != 0 || credit_base_cents != 0 {
        postings.insert(
            0,
            NewPosting {
                customer_id: Some(invoice.customer_id.as_str().to_owned()),
                ..NewPosting::new(accounts.ar.clone(), credit_cents, credit_base_cents)
            },
        );
    }
    if postings.len() < 2 {
        return Err(StoreError::Validation(
            "an invoice whose lines cancel out has nothing to book".to_owned(),
        ));
    }

    Ok(NewEntry {
        entry_date,
        kind: EntryKind::Invoice,
        source: Some(EntrySource {
            kind: SourceKind::Invoice,
            id: invoice.id.as_str().to_owned(),
            event: SourceEvent::Issue,
        }),
        // The number, and nothing a human typed: a memo is read on a journal
        // screen and printed in a CSV, and a customer's name is theirs.
        memo: invoice.number.clone().unwrap_or_default(),
        reverses_entry_id: None,
        attachment_node_id: None,
        currency: invoice.currency.clone(),
        fx,
        postings,
    })
}

/// The rate an issued document is booked at: the snapshot frozen on it, or the
/// identity when it was raised in the currency the books are kept in.
///
/// The two refusals are the ones that would otherwise put a wrong number into
/// the books silently. A **foreign-currency document with no snapshot** is one
/// issued before B1.21 existed: converting it at today's rate would restate a
/// past supply at a rate nobody applied (art. 91 fixes it at the tax point), so
/// it is refused and belongs in the opening balances an accountant writes. A
/// snapshot taken against a **different accounting currency** means the tenant
/// changed the currency they keep books in after issuing; its amounts cannot be
/// added to this ledger without a decision no rule is entitled to make.
fn booking_rate(
    document: &InvoiceDocument,
    base_currency: &str,
    entry_date: time::Date,
) -> Result<FxSnapshot> {
    let currency = document.invoice.currency.as_str();
    match document.invoice.fx.as_ref() {
        Some(fx) if fx.base_currency == base_currency => Ok(fx.clone()),
        Some(_) => Err(StoreError::Validation(
            "the invoice was converted into a currency the books are no longer kept in; \
             it cannot be booked automatically"
                .to_owned(),
        )),
        None if currency == base_currency => Ok(FxSnapshot::identity(base_currency, entry_date)),
        None => Err(StoreError::Validation(
            "the invoice carries no exchange rate, so its foreign-currency amounts cannot be \
             restated into the accounting currency"
                .to_owned(),
        )),
    }
}

/// Adds one figure into a running sum, refusing an overflow rather than
/// building an entry that balances against a wrapped number. Unreachable for a
/// validated document (`billing_totals` bounds every line), and total anyway.
fn add(running: i64, value: i64) -> Result<i64> {
    running.checked_add(value).ok_or_else(|| {
        StoreError::Validation("the invoice's amounts are too large to book".to_owned())
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::billing_totals::{Totals, VatSubtotal, totals};
    use crate::id::{BillingCustomerId, BillingInvoiceId, BillingLineId};
    use crate::{Invoice, Line, LineFigures};
    use time::{Date, Month, OffsetDateTime};

    fn day(day: u8) -> Date {
        Date::from_calendar_date(2026, Month::March, day).unwrap_or(Date::MIN)
    }

    /// A posting as the golden compares them: the account, the two money
    /// columns, and the dimensions a report groups by.
    type Row<'a> = (&'a str, i64, i64, Option<i32>, Option<&'a str>);

    fn accounts() -> InvoiceAccounts {
        InvoiceAccounts {
            ar: FinAccountId::new("acc-ar"),
            revenue: FinAccountId::new("acc-revenue"),
            vat_output: FinAccountId::new("acc-vat"),
        }
    }

    fn line(order: i32, qty_milli: i64, unit_price_cents: i64, vat_rate_bp: i32) -> Line {
        Line {
            id: BillingLineId::new(format!("line-{order}")),
            line_order: order,
            description: format!("Line {order}"),
            unit: "hour".to_owned(),
            qty_milli,
            unit_price_cents,
            vat_rate_bp,
        }
    }

    /// A document as the store reads it back: the header, the lines, and the
    /// totals derived from those lines — never totals a test made up.
    fn document(currency: &str, fx: Option<FxSnapshot>, lines: Vec<Line>) -> InvoiceDocument {
        let figures: Vec<LineFigures> = lines.iter().map(Line::figures).collect();
        let now = OffsetDateTime::UNIX_EPOCH;
        InvoiceDocument {
            invoice: Invoice {
                id: BillingInvoiceId::new("inv-1"),
                customer_id: BillingCustomerId::new("cust-1"),
                status: InvoiceStatus::Issued,
                currency: currency.to_owned(),
                number: Some("INV-2026-00007".to_owned()),
                issue_date: Some(day(4)),
                due_date: Some(day(25)),
                payment_terms_days: 21,
                is_credit_note: false,
                credits_invoice_id: None,
                quote_id: None,
                schedule_id: None,
                schedule_due_date: None,
                reference: String::new(),
                note: String::new(),
                fx,
                created_by: "user-1".to_owned(),
                created_at: now,
                updated_at: now,
            },
            totals: totals(&figures),
            lines,
            paid_cents: 0,
        }
    }

    /// The everyday document: €1 000 of consulting at 21 % and €200 of books at
    /// 9 %, in the currency the books are kept in.
    fn two_rate_document() -> InvoiceDocument {
        document(
            "EUR",
            Some(FxSnapshot::identity("EUR", day(4))),
            vec![
                line(0, 10_000, 10_000, 2100),
                line(1, 4_000, 5_000, 900),
                // A discount is a negative *quantity*: `billing_line` refuses a
                // negative price, so a document this shape is one the store can
                // actually produce.
                line(2, -1_000, 10_000, 2100),
            ],
        )
    }

    fn conflict<T: std::fmt::Debug>(result: Result<T>) -> String {
        match result {
            Err(StoreError::Conflict(message)) => message,
            other => panic!("expected Conflict, got {other:?}"),
        }
    }

    fn invalid<T: std::fmt::Debug>(result: Result<T>) -> String {
        match result {
            Err(StoreError::Validation(message)) => message,
            other => panic!("expected Validation, got {other:?}"),
        }
    }

    /// **The golden.** Written out the way an accountant writes an entry, from
    /// the arithmetic and not from the code under test:
    ///
    /// ```text
    /// 21 % net  10 h × 100.00 − 1 × 100.00 discount = 900.00 → VAT 189.00
    ///  9 % net   4 × 50.00                          = 200.00 → VAT  18.00
    /// gross                                         = 1 307.00
    ///
    /// debit   ar          1 307.00   customer cust-1
    /// credit  revenue       200.00   rate  900
    /// credit  vat_output     18.00   rate  900
    /// credit  revenue       900.00   rate 2100
    /// credit  vat_output    189.00   rate 2100
    /// ```
    #[test]
    fn an_issued_invoice_books_the_golden_entry() {
        let document = two_rate_document();
        let entry = invoice_issue_entry(&document, "EUR", &accounts())
            .unwrap_or_else(|err| panic!("refused: {err}"));

        assert_eq!(entry.kind, EntryKind::Invoice);
        assert_eq!(entry.entry_date, day(4), "the document's date, never today");
        assert_eq!(entry.memo, "INV-2026-00007");
        assert_eq!(entry.currency, "EUR");
        let source = entry.source.as_ref().unwrap_or_else(|| panic!("a source"));
        assert_eq!(source.kind, SourceKind::Invoice);
        assert_eq!(source.id, "inv-1");
        assert_eq!(source.event, SourceEvent::Issue);

        let written: Vec<Row<'_>> = entry
            .postings
            .iter()
            .map(|posting| {
                (
                    posting.account_id.as_str(),
                    posting.amount_cents,
                    posting.base_cents,
                    posting.vat_rate_bp,
                    posting.customer_id.as_deref(),
                )
            })
            .collect();
        assert_eq!(
            written,
            vec![
                ("acc-ar", 130_700, 130_700, None, Some("cust-1")),
                ("acc-revenue", -20_000, -20_000, Some(900), None),
                ("acc-vat", -1_800, -1_800, Some(900), None),
                ("acc-revenue", -90_000, -90_000, Some(2100), None),
                ("acc-vat", -18_900, -18_900, Some(2100), None),
            ]
        );
    }

    /// **P3**, stated in the shape `docs/design/finance.md` states it: the
    /// receivable is `billing_totals`' gross *exactly*, and the per-rate
    /// postings are that struct's own rows. If the ledger ever starts
    /// recomputing money, this is what goes red.
    #[test]
    fn the_entry_is_billing_totals_and_nothing_recomputed() {
        for document in [
            two_rate_document(),
            document(
                "EUR",
                Some(FxSnapshot::identity("EUR", day(4))),
                vec![line(0, 333, 999, 0), line(1, 7_777, 1_234, 500)],
            ),
        ] {
            let entry = invoice_issue_entry(&document, "EUR", &accounts())
                .unwrap_or_else(|err| panic!("refused: {err}"));
            let Totals {
                gross_cents,
                vat_by_rate,
                ..
            } = &document.totals;

            assert_eq!(entry.postings[0].amount_cents, *gross_cents);
            for VatSubtotal {
                rate_bp,
                net_cents,
                vat_cents,
            } in vat_by_rate
            {
                let at_rate: Vec<&NewPosting> = entry
                    .postings
                    .iter()
                    .filter(|posting| posting.vat_rate_bp == Some(*rate_bp))
                    .collect();
                let revenue: i64 = at_rate
                    .iter()
                    .filter(|posting| posting.account_id.as_str() == "acc-revenue")
                    .map(|posting| -posting.amount_cents)
                    .sum();
                let tax: i64 = at_rate
                    .iter()
                    .filter(|posting| posting.account_id.as_str() == "acc-vat")
                    .map(|posting| -posting.amount_cents)
                    .sum();
                assert_eq!(revenue, *net_cents, "revenue at {rate_bp} bp");
                assert_eq!(tax, *vat_cents, "output tax at {rate_bp} bp");
            }
            // And it balances, in the column the entry is denominated in.
            assert_eq!(
                entry
                    .postings
                    .iter()
                    .map(|posting| posting.amount_cents)
                    .sum::<i64>(),
                0
            );
        }
    }

    /// A foreign-currency document: every part is crossed at the frozen rate,
    /// the receivable is the sum of those crossed parts, and the entry balances
    /// in **both** columns without a rounding posting.
    ///
    /// The golden, hand-computed at 1 EUR = 1.0880 USD — a rate chosen because
    /// the two ways of doing it **disagree**, which is the only kind of example
    /// that can prove which one the rule does:
    ///
    /// ```text
    /// 21 % net  $900.00 → €827.21      VAT $189.00 → €173.71
    ///  9 % net  $200.00 → €183.82      VAT  $18.00 →  €16.54
    /// receivable $1 307.00 → €1 201.28  (the four rows added)
    ///                        €1 201.29  (the gross crossed — the wrong answer)
    /// ```
    #[test]
    fn a_foreign_currency_invoice_crosses_the_parts_and_sums_them() {
        let fx = FxSnapshot {
            base_currency: "EUR".to_owned(),
            rate_micro: 1_088_000,
            rate_date: day(3),
        };
        let mut document = two_rate_document();
        document.invoice.currency = "USD".to_owned();
        document.invoice.fx = Some(fx.clone());

        let entry = invoice_issue_entry(&document, "EUR", &accounts())
            .unwrap_or_else(|err| panic!("refused: {err}"));
        assert_eq!(entry.currency, "USD");
        assert_eq!(entry.fx, fx);

        let base: Vec<i64> = entry
            .postings
            .iter()
            .map(|posting| posting.base_cents)
            .collect();
        assert_eq!(base, vec![120_128, -18_382, -1_654, -82_721, -17_371]);
        assert_eq!(
            entry.postings[0].amount_cents, 130_700,
            "the document column is still the dollars the customer owes"
        );
        // The assertion that makes the one above mean something: crossing the
        // gross gives a *different* number here, so a rule that took that
        // shortcut would fail this test rather than pass it by luck.
        assert_eq!(
            convert_cents(130_700, fx.rate_micro),
            Some(120_129),
            "the whole, crossed, is a cent more than its parts"
        );
        for column in [
            entry
                .postings
                .iter()
                .map(|posting| posting.amount_cents)
                .sum::<i64>(),
            base.iter().sum::<i64>(),
        ] {
            assert_eq!(column, 0, "both columns balance with no residual");
        }
        // The books' figure for this document is the one the document itself
        // prints (`billing_fx::restated_into`) — to the cent.
        let printed = crate::billing_fx::restated_into("EUR", Some(&fx), &document.totals)
            .unwrap_or_else(|| panic!("a usable rate restates"));
        assert_eq!(entry.postings[0].base_cents, printed.gross_cents);
    }

    /// A rate group that nets to nothing writes no posting, and a document
    /// whose lines cancel out entirely books nothing at all rather than an
    /// entry of zeros.
    #[test]
    fn nothing_is_written_for_money_that_does_not_move() {
        let cancelling = document(
            "EUR",
            Some(FxSnapshot::identity("EUR", day(4))),
            vec![
                line(0, 1_000, 10_000, 2100),
                line(1, -1_000, 10_000, 2100),
                line(2, 2_000, 5_000, 900),
            ],
        );
        let entry = invoice_issue_entry(&cancelling, "EUR", &accounts())
            .unwrap_or_else(|err| panic!("refused: {err}"));
        assert_eq!(
            entry.postings.len(),
            3,
            "the 21 % group nets to zero and is not a posting"
        );
        assert!(
            entry
                .postings
                .iter()
                .all(|posting| posting.vat_rate_bp != Some(2100))
        );

        let empty = document(
            "EUR",
            Some(FxSnapshot::identity("EUR", day(4))),
            vec![line(0, 1_000, 10_000, 2100), line(1, -1_000, 10_000, 2100)],
        );
        assert!(
            invalid(invoice_issue_entry(&empty, "EUR", &accounts())).contains("nothing to book")
        );
    }

    /// Only a document that is an event books at issue, and the refusal says
    /// which rule owns the ones that do not.
    #[test]
    fn a_draft_a_void_and_a_credit_note_are_refused_by_this_rule() {
        let mut draft = two_rate_document();
        draft.invoice.status = InvoiceStatus::Draft;
        assert!(conflict(invoice_issue_entry(&draft, "EUR", &accounts())).contains("draft"));

        let mut void = two_rate_document();
        void.invoice.status = InvoiceStatus::Void;
        assert!(conflict(invoice_issue_entry(&void, "EUR", &accounts())).contains("void"));

        let mut credit = two_rate_document();
        credit.invoice.is_credit_note = true;
        assert!(conflict(invoice_issue_entry(&credit, "EUR", &accounts())).contains("credit note"));

        // A settled document is still bookable: it was issued first, and a
        // backfill meets documents that have since been paid.
        let mut paid = two_rate_document();
        paid.invoice.status = InvoiceStatus::Paid;
        assert!(invoice_issue_entry(&paid, "EUR", &accounts()).is_ok());
    }

    /// The rate refusals: a foreign document with no snapshot, and one taken
    /// against a different accounting currency. Both would otherwise book a
    /// number nobody applied.
    #[test]
    fn a_document_that_cannot_be_restated_is_refused() {
        let mut unconverted = two_rate_document();
        unconverted.invoice.currency = "USD".to_owned();
        unconverted.invoice.fx = None;
        assert!(
            invalid(invoice_issue_entry(&unconverted, "EUR", &accounts()))
                .contains("no exchange rate")
        );

        let mut other_books = two_rate_document();
        other_books.invoice.fx = Some(FxSnapshot {
            base_currency: "CHF".to_owned(),
            rate_micro: 950_000,
            rate_date: day(3),
        });
        assert!(
            invalid(invoice_issue_entry(&other_books, "EUR", &accounts()))
                .contains("no longer kept in")
        );

        // A document in the accounting currency issued before snapshots existed
        // needs no rate at all: it converts at the identity, on its own date.
        let mut old = two_rate_document();
        old.invoice.fx = None;
        let entry = invoice_issue_entry(&old, "EUR", &accounts())
            .unwrap_or_else(|err| panic!("refused: {err}"));
        assert_eq!(entry.fx, FxSnapshot::identity("EUR", day(4)));

        let mut undated = two_rate_document();
        undated.invoice.issue_date = None;
        assert!(invalid(invoice_issue_entry(&undated, "EUR", &accounts())).contains("issue date"));
    }

    #[test]
    fn adding_up_refuses_to_wrap() {
        assert!(add(i64::MAX, 1).is_err());
        assert_eq!(add(7, -7).unwrap_or(1), 0);
    }
}
