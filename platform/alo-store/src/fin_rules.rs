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
//! construction, so the invoice rule never needs the `rounding` account. And
//! the receivable the books carry is **exactly** the figure
//! [`crate::billing_fx::restated_into`] reports for the same document, which is
//! the figure the document prints and the VAT report sums; a ledger that
//! disagreed with the paper by a cent per invoice would be discovered a year
//! later by somebody reconciling both.
//!
//! The settlement rule (B4.04b) is the one whose two money postings are crossed
//! at **different** rates — the invoice's and the payment day's — and every
//! cent of that difference is an exchange difference with `fx_diff` as its
//! home, so it does not need `rounding` either. That account is still waiting
//! for the rule that genuinely produces an arithmetic residual (the note names
//! it as a general possibility; no rule written so far has one).

use crate::billing_fx::{FxSnapshot, convert_cents, convert_totals};
use crate::billing_invoices::{InvoiceDocument, InvoiceStatus};
use crate::billing_payments::Payment;
use crate::error::{Result, StoreError};
use crate::fin_accounts::AccountRole;
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
            let base = cross(cents, fx.rate_micro)?;
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

/// The accounts a payment's rule needs, resolved before it is called.
///
/// `fx_diff` is optional because it is needed exactly when the document is in a
/// currency the books are not kept in — see
/// [`settlement_needs_exchange_account`], which is the question the caller asks
/// before deciding whether a chart missing that role should refuse the payment.
#[derive(Debug, Clone)]
pub struct PaymentAccounts {
    /// Where the money landed — `bank` or `cash`, by
    /// [`payment_settlement_role`].
    pub settled_into: FinAccountId,
    /// Trade receivables — the credit: what the customer no longer owes.
    pub ar: FinAccountId,
    /// Foreign-exchange differences, for the base-column figure a settlement at
    /// a different rate leaves behind.
    pub fx_diff: Option<FinAccountId>,
}

/// Payment methods that mean physical cash, normalised the way
/// [`payment_settlement_role`] normalises the caller's word.
///
/// The languages the product ships in (en/fr/nl) plus German, because a method
/// is typed by whoever recorded the payment and B1 deliberately left it free
/// text ([`crate::billing_payments::PAYMENT_METHOD_MAX_CHARS`]).
const CASH_METHODS: &[&str] = &[
    "cash",
    "cash payment",
    "petty cash",
    "contant",
    "contante betaling",
    "kas",
    "especes",
    "espèces",
    "liquide",
    "numeraire",
    "numéraire",
    "bar",
    "bargeld",
    "barzahlung",
];

/// Which account a payment method settles into: `cash` for the words that mean
/// physical cash, `bank` for everything else.
///
/// **Whole-word equality, never a substring.** "cashless" and "non-cash" both
/// contain "cash" and both mean the bank, and a rule that read them the other
/// way would file real money into petty cash without anybody noticing until a
/// count. An unknown word falls to `bank`, which is where a transfer, a card
/// settlement and a direct debit all genuinely land — the default is the
/// common case, not a guess.
///
/// `docs/design/finance.md` promises a **per-tenant method map** eventually.
/// This is that map's closed default, and the tenant-editable table replaces
/// it (same signature, one lookup earlier) when the Accounts screen grows a
/// place to edit it — no rule above this function has to change.
pub fn payment_settlement_role(method: &str) -> AccountRole {
    let normalized = method
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase();
    if CASH_METHODS.contains(&normalized.as_str()) {
        AccountRole::Cash
    } else {
        AccountRole::Bank
    }
}

/// Whether settling `document` can produce an exchange difference, and so
/// whether the chart must hold an `fx_diff` account before the payment books.
///
/// It can, exactly when the document is not in the accounting currency: with
/// one currency both legs cross at the identity, the difference is provably
/// zero, and a chart missing the role must not refuse an ordinary euro payment
/// over an account that rule will never touch.
pub fn settlement_needs_exchange_account(document: &InvoiceDocument, base_currency: &str) -> bool {
    document.invoice.currency != base_currency
}

/// The entry a **recorded payment** books: the money where it landed, against
/// the receivable it relieves.
///
/// ```text
/// debit   bank/cash  amount received
/// credit  ar         the receivable relieved   dimension: customer
/// (fx_diff)          the base-column difference, when the two cross differently
/// ```
///
/// **The two money legs are crossed at two different rates, on purpose.** The
/// bank leg is what the accounting currency actually received, so it crosses at
/// the rate of the day the money arrived (`settled_at`). The receivable leg has
/// to remove what the *invoice* put there, so it crosses at the rate frozen on
/// the document (EU VAT Directive art. 91: the tax point's rate is the
/// document's rate forever). The difference between the two is not an error to
/// absorb — it is the gain or loss the tenant made by being paid later, and it
/// is posted to `fx_diff` as its own line, with `amount_cents = 0` because no
/// dollar moved on account of it (`docs/design/finance.md`, "Two currencies").
///
/// **The receivable relieved is cumulative, not per payment.** `paid_before`
/// and the payment's own amount define a prefix of the document's payments, and
/// the relief is the difference between what the whole prefix relieves and what
/// the shorter one did. That is what makes a fully settled document's
/// receivable go to **exactly** zero in both columns: the last payment carries
/// the cent or two by which the crossed gross differs from the sum of crossed
/// parts the issue entry booked, instead of leaving a phantom receivable that
/// no aged-debtors report could ever explain and no payment could ever clear.
/// A partial payment relieves the plain crossed amount, so `outstanding` in the
/// books is `billing_payments::Settlement`'s outstanding, restated.
///
/// # Errors
/// [`StoreError::Conflict`] when the document is not one that can be settled —
/// a draft is owed by nobody, a void one was cancelled, and money moving
/// against a credit note is a refund, which is a different event.
/// [`StoreError::Validation`] when the payment does not belong to the document,
/// its amount is not positive, the document cannot be restated into the
/// accounting currency, a snapshot was taken against a different accounting
/// currency, an amount cannot be crossed, or the entry needs the `fx_diff`
/// account and none was resolved.
pub fn payment_settle_entry(
    payment: &Payment,
    document: &InvoiceDocument,
    paid_before_cents: i64,
    base_currency: &str,
    settled_at: &FxSnapshot,
    accounts: &PaymentAccounts,
) -> Result<NewEntry> {
    let invoice = &document.invoice;
    if payment.invoice_id != invoice.id {
        return Err(StoreError::Validation(
            "that payment was not recorded against this invoice".to_owned(),
        ));
    }
    if invoice.is_credit_note {
        return Err(StoreError::Conflict(
            "a credit note is money owed to the customer; a refund against it is not a payment"
                .to_owned(),
        ));
    }
    match invoice.status {
        InvoiceStatus::Issued | InvoiceStatus::Paid => {}
        InvoiceStatus::Draft => {
            return Err(StoreError::Conflict(
                "a draft invoice is owed by nobody, so nothing it received can be booked"
                    .to_owned(),
            ));
        }
        InvoiceStatus::Void => {
            return Err(StoreError::Conflict(
                "a void invoice was cancelled; money against it is not a settlement".to_owned(),
            ));
        }
    }
    if payment.amount_cents <= 0 || paid_before_cents < 0 {
        return Err(StoreError::Validation(
            "a payment settles a positive amount out of a non-negative running total".to_owned(),
        ));
    }
    if settled_at.base_currency != base_currency {
        return Err(StoreError::Validation(
            "the settlement rate was taken against a currency the books are not kept in".to_owned(),
        ));
    }

    // The invoice's own rate, with the same two refusals booking it had: a
    // document whose receivable cannot be restated has no receivable here to
    // relieve either.
    let invoice_fx = booking_rate(document, base_currency, payment.paid_on)?;
    let booked_base_cents = convert_totals(&document.totals, invoice_fx.rate_micro)
        .ok_or_else(|| {
            StoreError::Validation(
                "the invoice's exchange rate cannot restate it into the accounting currency"
                    .to_owned(),
            )
        })?
        .gross_cents;

    let received_base = cross(payment.amount_cents, settled_at.rate_micro)?;
    let relieved_base = receivable_relief_base(
        document.totals.gross_cents,
        booked_base_cents,
        paid_before_cents,
        add(paid_before_cents, payment.amount_cents)?,
        invoice_fx.rate_micro,
    )?;
    let difference = sub(relieved_base, received_base)?;

    let mut postings = vec![
        NewPosting::new(
            accounts.settled_into.clone(),
            payment.amount_cents,
            received_base,
        ),
        NewPosting {
            customer_id: Some(invoice.customer_id.as_str().to_owned()),
            // Credits are negative: the sign is the direction
            // (`docs/design/finance.md`, "Signed amounts").
            ..NewPosting::new(accounts.ar.clone(), -payment.amount_cents, -relieved_base)
        },
    ];
    if difference != 0 {
        let fx_diff = accounts.fx_diff.clone().ok_or_else(|| {
            StoreError::Validation(
                "settling this document leaves an exchange difference, and no account holds \
                 the role 'fx_diff'"
                    .to_owned(),
            )
        })?;
        postings.push(NewPosting::new(fx_diff, 0, difference));
    }

    Ok(NewEntry {
        entry_date: payment.paid_on,
        kind: EntryKind::Payment,
        source: Some(EntrySource {
            kind: SourceKind::Payment,
            id: payment.id.as_str().to_owned(),
            event: SourceEvent::Settle,
        }),
        // The document's number, and nothing a human typed: the payment's own
        // reference is the bank's words about a named customer (law 1).
        memo: invoice.number.clone().unwrap_or_default(),
        reverses_entry_id: None,
        attachment_node_id: None,
        currency: invoice.currency.clone(),
        // The entry moved money on the day it moved, at that day's rate. The
        // invoice's rate does not vanish: it is what the receivable leg's base
        // amount was computed with, and the difference between the two is the
        // `fx_diff` line, which is where a reader looks for it.
        fx: settled_at.clone(),
        postings,
    })
}

/// How much of the receivable, in the accounting currency, a payment taking the
/// document from `paid_before` to `paid_after` relieves.
///
/// The cumulative function is `crossed(paid)`, plus — once the document is
/// settled — the whole difference between the receivable the issue entry
/// actually booked (the crossed parts, summed) and the crossed gross. Written
/// as a difference of prefixes it **telescopes**: whatever order the payments
/// are booked in, and however many there are, the reliefs add up to exactly
/// `booked_base_cents` at the moment the document is settled, and to
/// `crossed(paid)` before that.
///
/// A document worth nothing or less carries no adjustment: `paid ≥ gross` is
/// true of zero against zero, and a document nobody owes anything on has no
/// receivable to correct ([`crate::billing_payments::Settlement::of`] takes the
/// same view of the same arithmetic).
///
/// # Errors
/// [`StoreError::Validation`] when a figure cannot be crossed or the sums
/// overflow.
fn receivable_relief_base(
    gross_cents: i64,
    booked_base_cents: i64,
    paid_before_cents: i64,
    paid_after_cents: i64,
    rate_micro: i64,
) -> Result<i64> {
    let settlement_adjustment = if gross_cents > 0 {
        sub(booked_base_cents, cross(gross_cents, rate_micro)?)?
    } else {
        0
    };
    let cumulative = |paid: i64| -> Result<i64> {
        let crossed = cross(paid, rate_micro)?;
        if gross_cents > 0 && paid >= gross_cents {
            add(crossed, settlement_adjustment)
        } else {
            Ok(crossed)
        }
    };
    sub(
        cumulative(paid_after_cents)?,
        cumulative(paid_before_cents)?,
    )
}

/// Crosses one figure into the accounting currency, refusing rather than
/// guessing when the snapshot cannot divide.
fn cross(cents: i64, rate_micro: i64) -> Result<i64> {
    convert_cents(cents, rate_micro).ok_or_else(|| {
        StoreError::Validation(
            "the document's exchange rate cannot restate it into the accounting currency"
                .to_owned(),
        )
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
/// validated document (`billing_totals` bounds every line, `billing_payments`
/// every payment), and total anyway.
fn add(running: i64, value: i64) -> Result<i64> {
    running.checked_add(value).ok_or_else(|| {
        StoreError::Validation("the document's amounts are too large to book".to_owned())
    })
}

/// Takes one figure off another, refusing an overflow for the same reason
/// [`add`] does.
fn sub(running: i64, value: i64) -> Result<i64> {
    running.checked_sub(value).ok_or_else(|| {
        StoreError::Validation("the document's amounts are too large to book".to_owned())
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::billing_totals::{Totals, VatSubtotal, totals};
    use crate::id::{BillingCustomerId, BillingInvoiceId, BillingLineId, BillingPaymentId};
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
        assert!(sub(i64::MIN, 1).is_err());
        assert_eq!(sub(7, 7).unwrap_or(1), 0);
    }

    // ---------------------------------------------------------------------
    // The settlement rule (B4.04b)
    // ---------------------------------------------------------------------

    fn payment_accounts() -> PaymentAccounts {
        PaymentAccounts {
            settled_into: FinAccountId::new("acc-bank"),
            ar: FinAccountId::new("acc-ar"),
            fx_diff: Some(FinAccountId::new("acc-fx")),
        }
    }

    fn payment(amount_cents: i64, on: u8) -> Payment {
        Payment {
            id: BillingPaymentId::new(format!("pay-{amount_cents}")),
            invoice_id: BillingInvoiceId::new("inv-1"),
            paid_on: day(on),
            amount_cents,
            method: "bank transfer".to_owned(),
            reference: "E2E-9911".to_owned(),
            created_by: "user-1".to_owned(),
            created_at: OffsetDateTime::UNIX_EPOCH,
        }
    }

    /// The account, the two money columns and the customer dimension — what a
    /// settlement's golden compares.
    type SettleRow<'a> = (&'a str, i64, i64, Option<&'a str>);

    fn settle_rows(entry: &NewEntry) -> Vec<SettleRow<'_>> {
        entry
            .postings
            .iter()
            .map(|posting| {
                (
                    posting.account_id.as_str(),
                    posting.amount_cents,
                    posting.base_cents,
                    posting.customer_id.as_deref(),
                )
            })
            .collect()
    }

    /// **The golden**, in the currency the books are kept in, where there is no
    /// exchange difference to have an opinion about:
    ///
    /// ```text
    /// debit   bank   1 307.00
    /// credit  ar     1 307.00   customer cust-1
    /// ```
    #[test]
    fn a_payment_books_the_money_where_it_landed() {
        let document = two_rate_document();
        let received = payment(130_700, 20);
        let entry = payment_settle_entry(
            &received,
            &document,
            0,
            "EUR",
            &FxSnapshot::identity("EUR", day(20)),
            &payment_accounts(),
        )
        .unwrap_or_else(|err| panic!("refused: {err}"));

        assert_eq!(entry.kind, EntryKind::Payment);
        assert_eq!(entry.entry_date, day(20), "the day the money arrived");
        assert_eq!(entry.memo, "INV-2026-00007");
        assert_eq!(entry.currency, "EUR");
        let source = entry.source.as_ref().unwrap_or_else(|| panic!("a source"));
        assert_eq!(source.kind, SourceKind::Payment);
        assert_eq!(source.id, received.id.as_str());
        assert_eq!(source.event, SourceEvent::Settle);
        assert_eq!(
            settle_rows(&entry),
            vec![
                ("acc-bank", 130_700, 130_700, None),
                ("acc-ar", -130_700, -130_700, Some("cust-1")),
            ],
            "one currency leaves no exchange difference to post"
        );
        assert!(!settlement_needs_exchange_account(&document, "EUR"));
    }

    /// Partial payments relieve exactly what arrived, and the receivable the
    /// books still carry is the outstanding `billing_payments` reports.
    #[test]
    fn partial_payments_relieve_exactly_what_arrived() {
        let document = two_rate_document();
        let gross = document.totals.gross_cents;
        let accounts = payment_accounts();
        let mut relieved = 0;
        let mut paid_before = 0;

        for (amount, outstanding) in [(30_000, 100_700), (60_000, 40_700), (40_700, 0)] {
            let received = payment(amount, 20);
            let entry = payment_settle_entry(
                &received,
                &document,
                paid_before,
                "EUR",
                &FxSnapshot::identity("EUR", day(20)),
                &accounts,
            )
            .unwrap_or_else(|err| panic!("refused: {err}"));
            assert_eq!(entry.postings.len(), 2);
            relieved -= entry.postings[1].amount_cents;
            paid_before += amount;
            assert_eq!(
                gross - relieved,
                outstanding,
                "the books' receivable is the document's outstanding"
            );
            assert_eq!(
                crate::billing_payments::Settlement::of(gross, paid_before).outstanding_cents,
                outstanding,
                "and billing says the same number about the same document"
            );
        }
    }

    /// **The exchange difference.** A $1 307.00 invoice frozen at 1 EUR =
    /// 1.0880 USD, paid in two instalments at 1.1000 and 1.0500, hand-computed:
    ///
    /// ```text
    /// booked receivable (the crossed parts, summed)            €1 201.28
    ///
    /// payment 1  $500.00 @ 1.1000 → bank €454.55
    ///                    @ 1.0880 → ar   €459.56   fx_diff  €5.01 debit (loss)
    /// payment 2  $807.00 @ 1.0500 → bank €768.57
    ///            the rest of the receivable  €741.72   fx_diff €26.85 credit (gain)
    ///
    /// relieved   €459.56 + €741.72 = €1 201.28 — the receivable, to the cent
    /// ```
    #[test]
    fn a_foreign_currency_settlement_posts_the_exchange_difference() {
        let invoice_fx = FxSnapshot {
            base_currency: "EUR".to_owned(),
            rate_micro: 1_088_000,
            rate_date: day(3),
        };
        let mut document = two_rate_document();
        document.invoice.currency = "USD".to_owned();
        document.invoice.fx = Some(invoice_fx.clone());
        assert!(settlement_needs_exchange_account(&document, "EUR"));
        let accounts = payment_accounts();
        let booked = convert_totals(&document.totals, invoice_fx.rate_micro)
            .unwrap_or_else(|| panic!("a usable rate restates"))
            .gross_cents;
        assert_eq!(
            booked, 120_128,
            "what the issue entry put on the receivable"
        );

        let expected = [
            (
                50_000_i64,
                1_100_000_i64,
                vec![
                    ("acc-bank", 50_000_i64, 45_455_i64, None),
                    ("acc-ar", -50_000, -45_956, Some("cust-1")),
                    ("acc-fx", 0, 501, None),
                ],
            ),
            (
                80_700,
                1_050_000,
                vec![
                    ("acc-bank", 80_700, 76_857, None),
                    ("acc-ar", -80_700, -74_172, Some("cust-1")),
                    ("acc-fx", 0, -2_685, None),
                ],
            ),
        ];

        let mut paid_before = 0;
        let mut relieved_base = 0;
        for (amount, rate_micro, rows) in expected {
            let settled_at = FxSnapshot {
                base_currency: "EUR".to_owned(),
                rate_micro,
                rate_date: day(20),
            };
            let entry = payment_settle_entry(
                &payment(amount, 20),
                &document,
                paid_before,
                "EUR",
                &settled_at,
                &accounts,
            )
            .unwrap_or_else(|err| panic!("refused: {err}"));
            assert_eq!(settle_rows(&entry), rows);
            assert_eq!(entry.currency, "USD");
            assert_eq!(entry.fx, settled_at, "the rate the money actually moved at");
            for column in [
                entry
                    .postings
                    .iter()
                    .map(|posting| posting.amount_cents)
                    .sum::<i64>(),
                entry
                    .postings
                    .iter()
                    .map(|posting| posting.base_cents)
                    .sum::<i64>(),
            ] {
                assert_eq!(column, 0, "both columns balance");
            }
            relieved_base -= entry.postings[1].base_cents;
            paid_before += amount;
        }
        assert_eq!(
            relieved_base, booked,
            "the settled document's receivable is exactly zero in the base column too — \
             the crossed gross alone would leave a cent behind"
        );
        assert_eq!(
            convert_cents(document.totals.gross_cents, invoice_fx.rate_micro),
            Some(120_129),
            "and that cent is real: the whole crossed is not the parts crossed"
        );
    }

    /// The method map: the words that mean cash, and everything else.
    #[test]
    fn the_method_map_reads_cash_as_cash_and_the_rest_as_the_bank() {
        for method in ["cash", "CASH", " Petty  Cash ", "Contant", "espèces", "bar"] {
            assert_eq!(
                payment_settlement_role(method),
                AccountRole::Cash,
                "{method}"
            );
        }
        for method in [
            "",
            "bank transfer",
            "SEPA direct debit",
            "card",
            // The substring trap: both of these are the bank.
            "cashless",
            "non-cash card",
        ] {
            assert_eq!(
                payment_settlement_role(method),
                AccountRole::Bank,
                "{method}"
            );
        }
    }

    /// Only money against a document that is owed books, and only against the
    /// document it was recorded on.
    #[test]
    fn a_draft_a_void_a_credit_note_and_a_stray_payment_are_refused() {
        let accounts = payment_accounts();
        let settle = |document: &InvoiceDocument, received: &Payment| {
            payment_settle_entry(
                received,
                document,
                0,
                "EUR",
                &FxSnapshot::identity("EUR", day(20)),
                &accounts,
            )
        };

        let mut draft = two_rate_document();
        draft.invoice.status = InvoiceStatus::Draft;
        assert!(conflict(settle(&draft, &payment(1_000, 20))).contains("draft"));

        let mut void = two_rate_document();
        void.invoice.status = InvoiceStatus::Void;
        assert!(conflict(settle(&void, &payment(1_000, 20))).contains("void"));

        let mut credit = two_rate_document();
        credit.invoice.is_credit_note = true;
        assert!(conflict(settle(&credit, &payment(1_000, 20))).contains("credit note"));

        // A settled document still books further money: that is how an
        // overpayment reaches the ledger honestly (B1.19's own rule).
        let mut paid = two_rate_document();
        paid.invoice.status = InvoiceStatus::Paid;
        assert!(settle(&paid, &payment(1_000, 20)).is_ok());

        let document = two_rate_document();
        let mut elsewhere = payment(1_000, 20);
        elsewhere.invoice_id = BillingInvoiceId::new("inv-2");
        assert!(invalid(settle(&document, &elsewhere)).contains("not recorded against"));

        let mut nothing = payment(1_000, 20);
        nothing.amount_cents = 0;
        assert!(invalid(settle(&document, &nothing)).contains("positive amount"));
    }

    /// The two refusals that would otherwise write a number nobody applied: a
    /// settlement rate against the wrong books, and an exchange difference with
    /// nowhere to go.
    #[test]
    fn a_settlement_that_cannot_be_expressed_is_refused() {
        let document = two_rate_document();
        let wrong_books = FxSnapshot {
            base_currency: "CHF".to_owned(),
            rate_micro: 950_000,
            rate_date: day(20),
        };
        assert!(
            invalid(payment_settle_entry(
                &payment(1_000, 20),
                &document,
                0,
                "EUR",
                &wrong_books,
                &payment_accounts(),
            ))
            .contains("not kept in")
        );

        let mut foreign = two_rate_document();
        foreign.invoice.currency = "USD".to_owned();
        foreign.invoice.fx = Some(FxSnapshot {
            base_currency: "EUR".to_owned(),
            rate_micro: 1_088_000,
            rate_date: day(3),
        });
        let no_fx_account = PaymentAccounts {
            fx_diff: None,
            ..payment_accounts()
        };
        assert!(
            invalid(payment_settle_entry(
                &payment(50_000, 20),
                &foreign,
                0,
                "EUR",
                &FxSnapshot {
                    base_currency: "EUR".to_owned(),
                    rate_micro: 1_100_000,
                    rate_date: day(20),
                },
                &no_fx_account,
            ))
            .contains("'fx_diff'")
        );

        // A foreign document with no snapshot has no receivable this rule can
        // relieve either — the same refusal booking it gave.
        let mut unconverted = two_rate_document();
        unconverted.invoice.currency = "USD".to_owned();
        unconverted.invoice.fx = None;
        assert!(
            invalid(payment_settle_entry(
                &payment(1_000, 20),
                &unconverted,
                0,
                "EUR",
                &FxSnapshot::identity("EUR", day(20)),
                &payment_accounts(),
            ))
            .contains("no exchange rate")
        );
    }
}
