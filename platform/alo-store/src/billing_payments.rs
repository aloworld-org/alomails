//! Payments received against an invoice (alo Billing, ADR 0035, wave B1.19),
//! reached through the account door like [`crate::billing_invoices`].
//!
//! A payment is a **fact that happened** — a date, an amount, how it arrived,
//! the bank's own reference — and there may be many against one document,
//! because a customer settling a large bill in instalments is ordinary B2B
//! behaviour. That is the whole reason this is a table and not a column.
//!
//! **The document's paid-state is derived from these rows, never independently
//! written.** [`crate::billing_invoices::InvoiceStatus::Paid`] is a
//! *projection*: this module recomputes it inside the same transaction that
//! inserts or removes a payment, holding the invoice's row lock, so the status
//! column can never drift from the ledger underneath it and no request can set
//! it. Everything finer than "settled or not" — how much has arrived, what is
//! still outstanding — is computed on read by [`Settlement`] and stored
//! nowhere.
//!
//! **"Partially paid" is deliberately not a status.** It is a fact about money,
//! not a state of the document: an invoice that is half paid is still an issued
//! invoice, still owed, still overdue when its date passes. Adding a fifth
//! value to a four-state legal document to express an arithmetic comparison
//! would put that comparison in a column that then has to be kept true forever.
//!
//! **Amounts are strictly positive.** A payment that "un-pays" a document is
//! not a negative payment: it is either the removal of a payment recorded
//! wrongly ([`AccountStore::delete_billing_payment`]) or, if the debt itself
//! changed, a credit note (B1.09). A ledger where money can arrive negatively
//! is one where a typo is indistinguishable from a refund.
//!
//! Tenancy is structural: every statement carries `tenant_id` from the handle,
//! the invoice link is re-checked under the same handle before anything is
//! written (a guessed id from another tenant is a [`StoreError::NotFound`]),
//! and the database backs that with a composite foreign key on
//! `(tenant_id, invoice_id)`.

use time::{Date, OffsetDateTime};

use crate::account::AccountStore;
use crate::billing_field::bounded;
use crate::billing_invoices::InvoiceStatus;
use crate::billing_line::{INVOICE_LINES, Line};
use crate::billing_totals::{LineFigures, totals};
use crate::error::{Result, StoreError};
use crate::id::{BillingInvoiceId, BillingPaymentId};

/// How the money arrived: "bank transfer", "SEPA direct debit", "card", "cash".
///
/// Free text rather than an enum on purpose — the set varies per member state
/// and per tenant, and B4 maps methods to ledger accounts through a per-tenant
/// table rather than a list hardcoded here.
pub const PAYMENT_METHOD_MAX_CHARS: usize = 60;
/// The bank's own reference for the movement (end-to-end id, statement line),
/// which is what reconciliation (B4.09) will match on.
pub const PAYMENT_REFERENCE_MAX_CHARS: usize = 140;
/// The largest single payment we accept, in integer cents: €10 000 000 000.00.
///
/// A typo guard with an arithmetic job, chosen the same way the unit-price
/// ceiling was ([`crate::billing_field::UNIT_PRICE_MAX_CENTS`]): it is four
/// orders of magnitude below `i64::MAX`, so summing every payment a document
/// could plausibly carry stays inside `i64` with room to spare.
pub const PAYMENT_MAX_CENTS: i64 = 1_000_000_000_000;

/// The columns every read of a payment selects, in `PaymentRow` order.
const PAYMENT_COLS: &str = "id, invoice_id, paid_on, amount_cents, method, reference, \
     created_by, created_at";

/// A payment as the caller states it.
#[derive(Debug, Clone, Default)]
pub struct NewPayment {
    /// The day the money arrived, as the bank states it. `None` means today
    /// according to the database — never a date the caller invents.
    pub paid_on: Option<Date>,
    /// Integer cents in the document's own currency, strictly positive.
    pub amount_cents: i64,
    /// How it arrived, free text.
    pub method: String,
    /// The bank's own reference for the movement.
    pub reference: String,
}

/// A stored payment.
#[derive(Debug, Clone)]
pub struct Payment {
    /// Opaque id, unique within the tenant.
    pub id: BillingPaymentId,
    /// The document this money settled.
    pub invoice_id: BillingInvoiceId,
    /// The day the money arrived, as the bank states it — not the day it was
    /// keyed in, which is `created_at`.
    pub paid_on: Date,
    /// Integer cents, strictly positive.
    pub amount_cents: i64,
    /// How it arrived.
    pub method: String,
    /// The bank's own reference.
    pub reference: String,
    /// The user who recorded it.
    pub created_by: String,
    /// When it was recorded.
    pub created_at: OffsetDateTime,
}

/// Where a document stands against the money that has arrived for it.
///
/// Three states rather than a boolean, because "nothing has arrived" and "some
/// has arrived" are different facts to a bookkeeper chasing a debt, and neither
/// is a state of the *document*: both are `issued`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PaymentState {
    /// No money has arrived against this document.
    Unpaid,
    /// Some has, but not all of it.
    PartiallyPaid,
    /// The whole gross has arrived (or more — see [`Settlement::of`]).
    Paid,
}

impl PaymentState {
    /// The value this state is reported as on the wire.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Unpaid => "unpaid",
            Self::PartiallyPaid => "partiallyPaid",
            Self::Paid => "paid",
        }
    }

    /// Whether the document is settled in full — the one bit the stored
    /// `status` column projects.
    pub fn is_settled(self) -> bool {
        matches!(self, Self::Paid)
    }
}

/// What a document is worth, what has arrived against it, and what is left.
///
/// Computed on every read from the lines and the payment rows; stored nowhere,
/// so it can never disagree with either.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Settlement {
    /// What the document is worth, in cents (its computed gross).
    pub gross_cents: i64,
    /// The sum of its payments, in cents.
    pub paid_cents: i64,
    /// `gross − paid`, in cents. **Negative when the customer overpaid**,
    /// which is deliberate: the figure a bookkeeper needs is what is actually
    /// left, including the direction.
    pub outstanding_cents: i64,
    /// Where the document stands.
    pub state: PaymentState,
}

impl Settlement {
    /// The settlement of a document worth `gross_cents` against `paid_cents`
    /// received.
    ///
    /// **Overpayment counts as paid.** A customer who transfers a euro too
    /// much has settled the debt; refusing to call it settled would leave the
    /// document owed forever and chase them for a negative amount. What is
    /// left is then negative in `outstanding_cents`, which is exactly the
    /// figure a refund or a credit against the next invoice starts from.
    ///
    /// A document worth **nothing or less** (an invoice whose discount lines
    /// cancel it out, or a credit note) is `Unpaid` until money actually
    /// arrives for it, never "settled by arithmetic": `paid ≥ gross` is true of
    /// zero against zero, and a document nobody has paid anything for must not
    /// report itself as paid.
    pub fn of(gross_cents: i64, paid_cents: i64) -> Self {
        let state = if paid_cents <= 0 {
            PaymentState::Unpaid
        } else if paid_cents >= gross_cents {
            PaymentState::Paid
        } else {
            PaymentState::PartiallyPaid
        };
        Self {
            gross_cents,
            paid_cents,
            outstanding_cents: gross_cents.saturating_sub(paid_cents),
            state,
        }
    }
}

/// Validates a payment amount: strictly positive, and inside
/// [`PAYMENT_MAX_CENTS`].
///
/// Zero is refused as firmly as a negative amount. A zero payment records that
/// nothing happened, which is what *not* recording a payment already says, and
/// it would sit in the ledger looking like a settled instalment.
fn amount_cents(value: i64) -> Result<i64> {
    if value <= 0 {
        return Err(StoreError::Validation(
            "a payment amount must be greater than zero".to_owned(),
        ));
    }
    if value > PAYMENT_MAX_CENTS {
        return Err(StoreError::Validation(format!(
            "a payment amount must be at most {PAYMENT_MAX_CENTS} cents"
        )));
    }
    Ok(value)
}

/// The guard recording a payment runs: money can only arrive against a
/// document the customer actually holds and actually owes.
///
/// A **draft** is refused — it carries no number, was never sent, and is owed
/// by nobody, so money against it is a keying mistake worth reporting. A
/// **void** one is refused because it was cancelled: if money nevertheless
/// arrived for it, the document that describes what was bought has to be raised
/// again, not resurrected. A **paid** one still accepts money, because that is
/// how an overpayment or a duplicate transfer is recorded honestly rather than
/// hidden.
///
/// # Errors
/// [`StoreError::Conflict`] naming the status that refused, which the route
/// edge maps to `409`.
fn ensure_payable(status: InvoiceStatus) -> Result<()> {
    match status {
        InvoiceStatus::Issued | InvoiceStatus::Paid => Ok(()),
        InvoiceStatus::Draft => Err(StoreError::Conflict(
            "a draft invoice is owed by nobody; issue it before recording money against it"
                .to_owned(),
        )),
        InvoiceStatus::Void => Err(StoreError::Conflict(
            "a void invoice was cancelled; money cannot be recorded against it".to_owned(),
        )),
    }
}

/// One of a document's payments, with the sum of the payments that come before
/// it — the pair the settlement rule ([`crate::fin_rules::payment_settle_entry`])
/// needs to know how much receivable this payment relieves.
///
/// `payments` is [`AccountStore::billing_payments`]' own order, and the walk is
/// back to front. What the rule needs is not a particular sequence but a
/// **stable** one: the reliefs telescope to the booked receivable as long as
/// every payment of a document agrees about which ones precede it, whatever
/// order they were actually booked in. Pure, so the two callers that must agree
/// — booking a payment that was keyed in, and booking one a bank match just
/// created — agree by construction.
///
/// # Errors
/// [`StoreError::NotFound`] when the payment is not one of this document's,
/// which includes a document that is another tenant's (it reads as an empty
/// list); [`StoreError::Validation`] when the payments cannot be added up.
pub(crate) fn payment_in_sequence(
    payments: Vec<Payment>,
    payment_id: &BillingPaymentId,
) -> Result<(Payment, i64)> {
    let mut paid_before_cents: i64 = 0;
    for payment in payments.into_iter().rev() {
        if payment.id == *payment_id {
            return Ok((payment, paid_before_cents));
        }
        paid_before_cents = paid_before_cents
            .checked_add(payment.amount_cents)
            .ok_or_else(|| {
                StoreError::Validation(
                    "this document's payments are too large to add up".to_owned(),
                )
            })?;
    }
    Err(StoreError::NotFound)
}

impl AccountStore {
    /// Records money received against one of this tenant's **issued** invoices,
    /// and reprojects the document's status from the ledger that results.
    ///
    /// Everything happens in **one transaction**, under the invoice's row lock:
    /// the document is re-read there (so a void that raced this call either
    /// lands first and the payment is refused, or waits), the payment is
    /// written, and the status is recomputed from every payment row that then
    /// exists. Two payments arriving at once therefore serialise, and the
    /// second one sees the first — a document cannot be left `issued` because
    /// two half-payments each thought they were alone.
    ///
    /// A **credit note** is refused: it is money we owe the customer, not money
    /// they owe us, and a refund paid out is a movement in the other direction
    /// that belongs in the ledger (B4), not in this table pretending to settle
    /// a debt.
    ///
    /// The payment date defaults to **today according to the database**, and a
    /// date in the future is refused: money that has not arrived yet is not a
    /// payment. A date *before* the issue date is allowed — a deposit taken in
    /// advance is real, and the document it settles is raised afterwards.
    ///
    /// # Errors
    /// [`StoreError::NotFound`] when the invoice is absent or another tenant's;
    /// [`StoreError::Conflict`] when it is a draft, void, or a credit note;
    /// [`StoreError::Validation`] when the amount is not positive, is beyond
    /// [`PAYMENT_MAX_CENTS`], the date is in the future, or a text field breaks
    /// its bound; [`StoreError::Db`] on failure.
    pub async fn record_billing_payment(
        &self,
        invoice_id: &BillingInvoiceId,
        input: &NewPayment,
    ) -> Result<BillingPaymentId> {
        let mut tx = self.pool.begin().await.map_err(StoreError::Db)?;
        let id = self
            .record_billing_payment_in(&mut tx, invoice_id, input)
            .await?;
        tx.commit().await.map_err(StoreError::Db)?;
        Ok(id)
    }

    /// [`AccountStore::record_billing_payment`], inside a transaction the
    /// caller owns.
    ///
    /// Money arriving is rarely only money arriving. A confirmed bank match
    /// ([`crate::bank_reconcile`]) records the payment, books the receivable it
    /// relieves and marks the statement line settled, and a tenant must never
    /// be left holding any one of those three without the others. Every rule
    /// and every refusal is the public door's; only the `BEGIN` and the
    /// `COMMIT` move to the caller.
    ///
    /// # Errors
    /// Exactly [`AccountStore::record_billing_payment`]'s. The caller must drop
    /// the transaction on any of them rather than carrying on.
    pub(crate) async fn record_billing_payment_in(
        &self,
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
        invoice_id: &BillingInvoiceId,
        input: &NewPayment,
    ) -> Result<BillingPaymentId> {
        let amount = amount_cents(input.amount_cents)?;
        let method = bounded("method", &input.method, PAYMENT_METHOD_MAX_CHARS)?;
        let reference = bounded("reference", &input.reference, PAYMENT_REFERENCE_MAX_CHARS)?;

        // Authoritative: the state that matters is the one under the lock the
        // reprojection below writes through. Dropping the transaction on any
        // error rolls it back untouched.
        let locked = self.lock_invoice_for_payment(tx, invoice_id).await?;
        if locked.is_credit_note {
            return Err(StoreError::Conflict(
                "a credit note is money owed to the customer; a refund is not recorded as a \
                 payment against it"
                    .to_owned(),
            ));
        }
        ensure_payable(locked.status)?;

        // One clock for the whole transaction, and the same clock the issue
        // date was read from.
        let today: Date = sqlx::query_scalar("SELECT CURRENT_DATE")
            .fetch_one(&mut **tx)
            .await
            .map_err(StoreError::Db)?;
        let paid_on = input.paid_on.unwrap_or(today);
        if paid_on > today {
            return Err(StoreError::Validation(
                "a payment cannot be dated in the future".to_owned(),
            ));
        }

        let id = BillingPaymentId::generate();
        sqlx::query(
            "INSERT INTO billing_payments (tenant_id, id, invoice_id, paid_on, amount_cents, \
                 method, reference, created_by) \
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8)",
        )
        .bind(self.tenant.as_str())
        .bind(id.as_str())
        .bind(invoice_id.as_str())
        .bind(paid_on)
        .bind(amount)
        .bind(&method)
        .bind(&reference)
        .bind(self.user.as_str())
        .execute(&mut **tx)
        .await
        .map_err(StoreError::Db)?;

        self.reproject_invoice_status(tx, invoice_id).await?;
        Ok(id)
    }

    /// Removes a payment recorded against one of this tenant's invoices, and
    /// reprojects the document's status from what is left.
    ///
    /// This is the correction path, and the only one: a mis-keyed amount is
    /// removed and re-entered, never patched, so the ledger reads as a list of
    /// movements that happened rather than a list of movements as last edited.
    /// A document that was settled by the removed payment goes back to
    /// `issued`, and becomes overdue again if its date has passed — which is
    /// the honest answer, since the money is not there.
    ///
    /// The payment is addressed **through its invoice**, so an id belonging to
    /// another document (or another tenant) is a `NotFound` rather than a
    /// deletion somewhere unexpected.
    ///
    /// # Errors
    /// [`StoreError::NotFound`] when the invoice or the payment is absent or
    /// another tenant's; [`StoreError::Db`] on failure.
    pub async fn delete_billing_payment(
        &self,
        invoice_id: &BillingInvoiceId,
        payment_id: &BillingPaymentId,
    ) -> Result<()> {
        let mut tx = self.pool.begin().await.map_err(StoreError::Db)?;
        // The invoice's lock is taken first and by every payment path, so
        // removals and insertions serialise on the same row and the status
        // below is computed from a ledger nobody else is changing.
        self.lock_invoice_for_payment(&mut tx, invoice_id).await?;
        let removed = sqlx::query(
            "DELETE FROM billing_payments \
             WHERE tenant_id = $1 AND invoice_id = $2 AND id = $3",
        )
        .bind(self.tenant.as_str())
        .bind(invoice_id.as_str())
        .bind(payment_id.as_str())
        .execute(&mut *tx)
        .await
        .map_err(StoreError::Db)?;
        if removed.rows_affected() == 0 {
            return Err(StoreError::NotFound);
        }
        self.reproject_invoice_status(&mut tx, invoice_id).await?;
        tx.commit().await.map_err(StoreError::Db)?;
        Ok(())
    }

    /// The payments recorded against one of this tenant's invoices, newest
    /// first.
    ///
    /// An id that is absent or another tenant's yields an empty list, like
    /// every other list read in billing — never an existence oracle.
    ///
    /// # Errors
    /// [`StoreError::Db`] on failure.
    pub async fn billing_payments(&self, invoice_id: &BillingInvoiceId) -> Result<Vec<Payment>> {
        self.billing_payments_on(&self.pool, invoice_id).await
    }

    /// [`AccountStore::billing_payments`] against any executor.
    ///
    /// A caller that has just inserted a payment inside its own transaction has
    /// to read the sequence **there**: the pool cannot see an uncommitted row,
    /// and the settlement rule's `paid_before` is a fact about the whole
    /// document, not about one insert ([`crate::bank_reconcile`]).
    ///
    /// # Errors
    /// [`StoreError::Db`] on failure.
    pub(crate) async fn billing_payments_on<'e, E>(
        &self,
        executor: E,
        invoice_id: &BillingInvoiceId,
    ) -> Result<Vec<Payment>>
    where
        E: sqlx::Executor<'e, Database = sqlx::Postgres>,
    {
        let rows = sqlx::query_as::<_, PaymentRow>(&format!(
            "SELECT {PAYMENT_COLS} FROM billing_payments \
             WHERE tenant_id = $1 AND invoice_id = $2 \
             ORDER BY paid_on DESC, created_at DESC, id"
        ))
        .bind(self.tenant.as_str())
        .bind(invoice_id.as_str())
        .fetch_all(executor)
        .await
        .map_err(StoreError::Db)?;
        Ok(rows.into_iter().map(PaymentRow::into_payment).collect())
    }

    /// Takes the invoice's row lock inside `tx` and returns the two stored
    /// facts a payment path decides against.
    ///
    /// It is a payment-shaped read rather than a reuse of the invoice module's
    /// own lock because the decisions differ: what matters here is where the
    /// document is in its life and whether it is a credit note, and nothing
    /// else about it.
    ///
    /// # Errors
    /// [`StoreError::NotFound`] when the id is absent **or another tenant's**;
    /// [`StoreError::Db`] on failure or on a status the code does not know.
    async fn lock_invoice_for_payment(
        &self,
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
        id: &BillingInvoiceId,
    ) -> Result<PayableInvoice> {
        let row: Option<(String, bool)> = sqlx::query_as(
            "SELECT status, is_credit_note FROM billing_invoices \
             WHERE tenant_id = $1 AND id = $2 FOR UPDATE",
        )
        .bind(self.tenant.as_str())
        .bind(id.as_str())
        .fetch_optional(&mut **tx)
        .await
        .map_err(StoreError::Db)?;
        let (status, is_credit_note) = row.ok_or(StoreError::NotFound)?;
        let status = InvoiceStatus::parse(&status).ok_or_else(|| {
            StoreError::Db(sqlx::Error::Decode(
                "billing_invoices.status is not a known status".into(),
            ))
        })?;
        Ok(PayableInvoice {
            status,
            is_credit_note,
        })
    }

    /// Recomputes the document's `status` from its lines and its payment rows,
    /// inside the transaction that just changed one of them and while its row
    /// lock is held.
    ///
    /// The column carries one bit of this — settled or not — and it is a
    /// projection, never an input: `paid` when the whole gross has arrived,
    /// `issued` when it has not. A `draft` or `void` document is left alone,
    /// because neither can carry payments in the first place, and because a
    /// projection must never be the thing that resurrects a cancelled document.
    ///
    /// # Errors
    /// [`StoreError::Db`] on failure.
    async fn reproject_invoice_status(
        &self,
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
        invoice_id: &BillingInvoiceId,
    ) -> Result<()> {
        let lines = INVOICE_LINES
            .read(&mut **tx, self.tenant.as_str(), invoice_id.as_str())
            .await?;
        let figures: Vec<LineFigures> = lines.iter().map(Line::figures).collect();
        let gross = totals(&figures).gross_cents;

        let paid: Option<i64> = sqlx::query_scalar(
            "SELECT sum(amount_cents)::bigint FROM billing_payments \
             WHERE tenant_id = $1 AND invoice_id = $2",
        )
        .bind(self.tenant.as_str())
        .bind(invoice_id.as_str())
        .fetch_one(&mut **tx)
        .await
        .map_err(StoreError::Db)?;

        let settled = Settlement::of(gross, paid.unwrap_or(0)).state.is_settled();
        let next = if settled {
            InvoiceStatus::Paid
        } else {
            InvoiceStatus::Issued
        };
        sqlx::query(
            "UPDATE billing_invoices SET status = $3, updated_at = now() \
             WHERE tenant_id = $1 AND id = $2 AND status IN ('issued', 'paid') AND status <> $3",
        )
        .bind(self.tenant.as_str())
        .bind(invoice_id.as_str())
        .bind(next.as_str())
        .execute(&mut **tx)
        .await
        .map_err(StoreError::Db)?;
        Ok(())
    }
}

// ---- row types --------------------------------------------------------------

/// What a payment path's locking read hands back.
#[derive(Debug)]
struct PayableInvoice {
    status: InvoiceStatus,
    is_credit_note: bool,
}

#[derive(sqlx::FromRow)]
struct PaymentRow {
    id: String,
    invoice_id: String,
    paid_on: Date,
    amount_cents: i64,
    method: String,
    reference: String,
    created_by: String,
    created_at: OffsetDateTime,
}

impl PaymentRow {
    fn into_payment(self) -> Payment {
        Payment {
            id: BillingPaymentId::new(self.id),
            invoice_id: BillingInvoiceId::new(self.invoice_id),
            paid_on: self.paid_on,
            amount_cents: self.amount_cents,
            method: self.method,
            reference: self.reference,
            created_by: self.created_by,
            created_at: self.created_at,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nothing_received_is_unpaid_whatever_the_document_is_worth() {
        for gross in [0, 1, 191_310, -50_000] {
            let s = Settlement::of(gross, 0);
            assert_eq!(s.state, PaymentState::Unpaid, "gross {gross}");
            assert_eq!(s.outstanding_cents, gross);
            assert!(!s.state.is_settled());
        }
    }

    #[test]
    fn part_of_the_gross_is_partially_paid_and_the_rest_is_still_owed() {
        let s = Settlement::of(191_310, 100_000);
        assert_eq!(s.state, PaymentState::PartiallyPaid);
        assert_eq!(s.outstanding_cents, 91_310);
        assert!(!s.state.is_settled(), "the document is still owed");
        // One cent short is still short: there is no tolerance band, because a
        // cent is exactly the kind of difference a bank charge leaves behind
        // and a bookkeeper must be able to see it.
        let nearly = Settlement::of(191_310, 191_309);
        assert_eq!(nearly.state, PaymentState::PartiallyPaid);
        assert_eq!(nearly.outstanding_cents, 1);
    }

    #[test]
    fn the_whole_gross_settles_the_document_and_more_than_it_still_does() {
        let exact = Settlement::of(191_310, 191_310);
        assert_eq!(exact.state, PaymentState::Paid);
        assert_eq!(exact.outstanding_cents, 0);
        assert!(exact.state.is_settled());
        // Overpayment: settled, and what is left is negative — the figure a
        // refund or a credit against the next invoice starts from.
        let over = Settlement::of(191_310, 200_000);
        assert_eq!(over.state, PaymentState::Paid);
        assert_eq!(over.outstanding_cents, -8_690);
    }

    #[test]
    fn a_worthless_document_is_never_settled_by_arithmetic() {
        // `paid >= gross` is true of 0 against 0 and of 0 against a negative
        // gross; neither means anybody paid anything.
        assert_eq!(Settlement::of(0, 0).state, PaymentState::Unpaid);
        assert_eq!(Settlement::of(-50_000, 0).state, PaymentState::Unpaid);
        // Money actually arriving against one does settle it.
        assert_eq!(Settlement::of(0, 100).state, PaymentState::Paid);
    }

    #[test]
    fn a_payment_amount_must_be_positive_and_bounded() {
        assert_eq!(amount_cents(1).ok(), Some(1));
        assert_eq!(
            amount_cents(PAYMENT_MAX_CENTS).ok(),
            Some(PAYMENT_MAX_CENTS)
        );
        for bad in [0, -1, -191_310] {
            match amount_cents(bad) {
                Err(StoreError::Validation(message)) => {
                    assert!(message.contains("greater than zero"), "{message}");
                }
                other => panic!("expected Validation for {bad}, got {other:?}"),
            }
        }
        match amount_cents(PAYMENT_MAX_CENTS + 1) {
            Err(StoreError::Validation(message)) => {
                assert!(message.contains("at most"), "{message}");
            }
            other => panic!("expected Validation, got {other:?}"),
        }
    }

    #[test]
    fn only_a_document_that_is_owed_accepts_money() {
        // A paid one still does: that is how an overpayment or a duplicate
        // transfer is recorded honestly rather than hidden.
        for payable in [InvoiceStatus::Issued, InvoiceStatus::Paid] {
            assert!(ensure_payable(payable).is_ok(), "{payable:?}");
        }
        let draft = match ensure_payable(InvoiceStatus::Draft) {
            Err(StoreError::Conflict(message)) => message,
            other => panic!("expected Conflict for a draft, got {other:?}"),
        };
        assert!(
            draft.contains("draft") && draft.contains("issue"),
            "{draft}"
        );
        let void = match ensure_payable(InvoiceStatus::Void) {
            Err(StoreError::Conflict(message)) => message,
            other => panic!("expected Conflict for a void document, got {other:?}"),
        };
        assert!(
            void.contains("void") || void.contains("cancelled"),
            "{void}"
        );
    }

    #[test]
    fn every_payment_state_has_one_stable_wire_name() {
        assert_eq!(PaymentState::Unpaid.as_str(), "unpaid");
        assert_eq!(PaymentState::PartiallyPaid.as_str(), "partiallyPaid");
        assert_eq!(PaymentState::Paid.as_str(), "paid");
    }
}
