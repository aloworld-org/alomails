//! Billing invoices — the document itself (alo Billing, ADR 0035, wave B1),
//! reached through the account door like [`crate::billing_customers`].
//!
//! An invoice is not a row that gets edited forever: it is a **draft** until
//! it is issued, and issuing draws the next number from the tenant's gapless
//! sequence ([`crate::billing_sequence`]), stamps the dates and freezes the
//! content. This module owns the document's whole life — creating the draft,
//! replacing its header, replacing its line set, deleting it while it is still
//! nothing, issuing it, voiding an issued one, and **crediting** one — and
//! reading a document back with its totals.
//!
//! **A credit note is an invoice, not a second kind of document.** It lives in
//! this table, draws from the same number series, and goes through the same
//! draft → issued life; what makes it one is that it names the document it
//! credits and carries that document's lines with their quantities negated. A
//! full mirror is worth exactly the negative of its original — the rounding
//! convention in [`crate::billing_totals`] is chosen so that holds to the cent
//! — so the two documents together sum to zero, which is what "this invoice was
//! corrected" has to mean in a ledger.
//!
//! **Issuing is the only transition that assigns a number, and it never lies
//! about the date.** The issue date is the day the store issued it, read from
//! the database's own clock inside the issuing transaction — not a date the
//! caller supplies. A number series whose numbers ascend while their dates do
//! not is not a gapless series in any sense a tax authority accepts, and
//! backdating is exactly how that happens; the due date follows from the terms
//! snapshotted on the document.
//!
//! **A document that is no longer a draft is frozen.** Every write here takes
//! the row's lock and re-reads its status inside the same transaction before
//! it touches anything, so an edit that raced an issue is refused rather than
//! applied to a numbered document — the check and the write cannot be
//! separated by another transaction. The refusal is a
//! [`StoreError::Conflict`] (a well-formed request that disagrees with the
//! document's state, `409` at the route edge), never a silent no-op, and it
//! outranks any complaint about what was sent: a frozen document refuses the
//! edit whatever the payload says. Deletion is a draft-only act for the same
//! reason — an issued document is voided (its number is kept so the sequence
//! stays gapless), never deleted.
//!
//! **Nothing here stores money it computed.** Net, VAT and gross are derived
//! from the lines on every read by [`crate::billing_totals`], so a total can
//! never drift from the lines that justify it, and no client can influence
//! what a document is worth by sending a number.
//!
//! Lines are written as a **whole set**, in the caller's order, inside one
//! transaction — a draft editor sends the document it wants, not a patch
//! stream, so there is no window in which a document is half-edited and no
//! ambiguity about line order.
//!
//! Tenancy is structural: every statement carries `tenant_id` from the handle,
//! the customer link is re-checked under the same handle before it is written
//! (a guessed id from another tenant is a [`StoreError::NotFound`], never a
//! cross-tenant link), and the database backs that with a composite foreign
//! key on `(tenant_id, customer_id)`.

use std::collections::HashMap;

use time::{Date, Duration, OffsetDateTime};

use sqlx::PgConnection;

use crate::account::AccountStore;
use crate::billing_customers::customer_read;
use crate::billing_field::{bounded, currency, payment_terms_days};
use crate::billing_fx::{FxSnapshot, restated};
use crate::billing_fx_rates::snapshot_at;
use crate::billing_line::{
    FiguresRow, INVOICE_LINES, Line, NewLine, NormalizedLine, group_figures, normalize_lines,
};
use crate::billing_payments::Settlement;
use crate::billing_sequence::{
    INVOICE_NUMBER_PREFIX, INVOICE_SEQUENCE_KIND, document_number, draw_next,
};
use crate::billing_settings::base_currency_in;
use crate::billing_totals::{LineFigures, Totals, totals};
use crate::error::{Result, StoreError};
use crate::fin_journal::{EntrySource, SourceEvent, SourceKind, reversal_entry};
use crate::id::{BillingCustomerId, BillingInvoiceId, BillingQuoteId, BillingScheduleId};
use crate::time_invoice::release_billed_hours;

/// The customer's own reference (a PO number, a cost centre) printed on the
/// document.
pub const INVOICE_REFERENCE_MAX_CHARS: usize = 120;
/// A free-text note printed under the lines (delivery terms, a thank-you, the
/// reverse-charge sentence).
pub const INVOICE_NOTE_MAX_CHARS: usize = 2_000;

/// The columns every read of an invoice selects, in `InvoiceRow` order.
const INVOICE_COLS: &str = "id, customer_id, status, currency, number, issue_date, due_date, \
     payment_terms_days, is_credit_note, credits_invoice_id, quote_id, schedule_id, \
     schedule_due_date, reference, note, fx_base_currency, fx_rate_micro, fx_rate_date, \
     created_by, created_at, updated_at";

/// Where a document is in its life.
///
/// The transitions are `draft → issued → paid` and `issued → void`; a draft is
/// deleted rather than voided, because it never consumed a number. Only a
/// draft is editable, enforced by [`InvoiceStatus::ensure_editable`] on every
/// write path; issuing itself arrives with B1.08.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum InvoiceStatus {
    /// Editable, unnumbered, not yet a legal document.
    Draft,
    /// Numbered, dated and frozen; owed by the customer.
    Issued,
    /// Settled in full by recorded payments (B1.19).
    Paid,
    /// Issued and then cancelled. The number is kept — the sequence stays
    /// gapless — and the document remains readable.
    Void,
}

impl InvoiceStatus {
    /// The value stored in the `status` column.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Draft => "draft",
            Self::Issued => "issued",
            Self::Paid => "paid",
            Self::Void => "void",
        }
    }

    /// Parses a stored status, or `None` if it is not one we know.
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "draft" => Some(Self::Draft),
            "issued" => Some(Self::Issued),
            "paid" => Some(Self::Paid),
            "void" => Some(Self::Void),
            _ => None,
        }
    }

    /// Whether the document is still editable.
    pub fn is_draft(self) -> bool {
        matches!(self, Self::Draft)
    }

    /// The guard every write path runs before it changes a document: a draft
    /// may be edited and deleted, anything else is frozen.
    ///
    /// Frozen means frozen for *all* of them — an issued invoice is a legal
    /// document that a customer, an accountant and a tax authority may already
    /// hold, so the store refuses the write rather than rewriting history and
    /// hoping nobody kept a copy. A `paid` document is equally frozen (it was
    /// issued first), and so is a `void` one: voiding cancels a document, it
    /// does not reopen it for editing.
    ///
    /// # Errors
    /// [`StoreError::Conflict`] naming the status that refused, which the
    /// route edge maps to `409`.
    pub fn ensure_editable(self) -> Result<()> {
        if self.is_draft() {
            return Ok(());
        }
        Err(StoreError::Conflict(format!(
            "an invoice can only be changed while it is a draft; this one is {}",
            self.as_str()
        )))
    }

    /// The guard issuing runs: only a draft can be issued.
    ///
    /// Re-issuing is refused rather than being a no-op, because the two
    /// answers mean different things to a caller that retried after a timeout:
    /// this one says "it already has a number", and the document it names
    /// carries that number.
    ///
    /// # Errors
    /// [`StoreError::Conflict`] naming the status that refused (`409`).
    pub fn ensure_issuable(self) -> Result<()> {
        if self.is_draft() {
            return Ok(());
        }
        Err(StoreError::Conflict(format!(
            "an invoice can only be issued while it is a draft; this one is {}",
            self.as_str()
        )))
    }

    /// The guard voiding runs: only an issued document can be voided.
    ///
    /// A draft is deleted instead (it never consumed a number), a void one is
    /// already void, and a **paid** one is not cancelled by fiat — money
    /// changed hands, so it is corrected with a credit note (B1.09) that
    /// leaves both documents standing.
    ///
    /// # Errors
    /// [`StoreError::Conflict`] naming the status that refused (`409`).
    pub fn ensure_voidable(self) -> Result<()> {
        if matches!(self, Self::Issued) {
            return Ok(());
        }
        Err(StoreError::Conflict(format!(
            "only an issued invoice can be voided; this one is {}",
            self.as_str()
        )))
    }

    /// The guard crediting runs: only a document the customer actually holds
    /// can be credited — `issued`, or `paid` once the money has arrived.
    ///
    /// A **draft** is refused: it carries no number, is owed by nobody, and is
    /// simply deleted, so a credit note against it would credit a document that
    /// legally never existed. A **void** one is refused for the mirror reason:
    /// voiding has already cancelled it in full, and crediting it as well would
    /// take the ledger below zero.
    ///
    /// # Errors
    /// [`StoreError::Conflict`] naming the status that refused (`409`).
    pub fn ensure_creditable(self) -> Result<()> {
        match self {
            Self::Issued | Self::Paid => Ok(()),
            Self::Draft => Err(StoreError::Conflict(
                "a draft invoice has no number and is owed by nobody; delete it instead of \
                 crediting it"
                    .to_owned(),
            )),
            Self::Void => Err(StoreError::Conflict(
                "a void invoice has already been cancelled in full; it cannot also be credited"
                    .to_owned(),
            )),
        }
    }
}

/// Turns a stored status string into a status, or reports corrupt data.
///
/// A status the code does not know is corrupt data, not user input: it is
/// reported as a decode failure (detail in the source, never in the message)
/// rather than guessed at, because guessing here would mean treating a frozen
/// document as editable.
fn parse_stored_status(stored: &str) -> Result<InvoiceStatus> {
    InvoiceStatus::parse(stored).ok_or_else(|| {
        StoreError::Db(sqlx::Error::Decode(
            "billing_invoices.status is not a known status".into(),
        ))
    })
}

/// The writable header of an invoice, used for both create and update (an
/// update is a full replace — the route layer merges a partial `PATCH` onto
/// the stored record before calling). Lines are written separately, as a set.
///
/// `currency` and `payment_terms_days` are `None` to mean *take the
/// customer's*, which is what a UI that has not asked the user should send.
/// Whatever is resolved is then **stored on the document**: changing a
/// customer's terms next year must not silently restate a document raised
/// this year.
#[derive(Debug, Clone)]
pub struct NewInvoice {
    /// The party billed. Must be one of this tenant's customers.
    pub customer_id: BillingCustomerId,
    /// ISO 4217 code, or `None` for the customer's default.
    pub currency: Option<String>,
    /// Days from issue to due, or `None` for the customer's terms.
    pub payment_terms_days: Option<i32>,
    /// The customer's own reference (PO number), printed on the document.
    pub reference: String,
    /// Free-text note printed under the lines.
    pub note: String,
}

impl NewInvoice {
    /// The blank header a new draft starts from: this customer, their
    /// currency and their terms, no reference and no note. There is
    /// deliberately no [`Default`] — an invoice without a customer is not a
    /// document, and a defaulted (empty) customer id would only fail later,
    /// further from the mistake.
    pub fn for_customer(customer_id: BillingCustomerId) -> Self {
        Self {
            customer_id,
            currency: None,
            payment_terms_days: None,
            reference: String::new(),
            note: String::new(),
        }
    }
}

/// The header of a stored invoice. Its money lives in [`Totals`], computed
/// from the lines.
#[derive(Debug, Clone)]
pub struct Invoice {
    /// Opaque id, unique within the tenant.
    pub id: BillingInvoiceId,
    /// The party billed.
    pub customer_id: BillingCustomerId,
    /// Where the document is in its life.
    pub status: InvoiceStatus,
    /// ISO 4217 code the document was raised in.
    pub currency: String,
    /// The legal document number, `None` while draft.
    pub number: Option<String>,
    /// Date of issue, `None` while draft.
    pub issue_date: Option<Date>,
    /// Date payment is due, `None` while draft.
    pub due_date: Option<Date>,
    /// Payment terms snapshotted from the customer, in days.
    pub payment_terms_days: i32,
    /// Whether this document credits another (B1.09).
    pub is_credit_note: bool,
    /// The document credited, when this is a credit note.
    pub credits_invoice_id: Option<BillingInvoiceId>,
    /// The accepted quote this draft was raised from (B1.12), when it came
    /// from one. Never writable from a request: it is stamped by the
    /// acceptance itself and stays for the life of the document.
    pub quote_id: Option<BillingQuoteId>,
    /// The recurring arrangement whose due run raised this draft (B2.11), when
    /// one did. Stamped by the run and never writable from a request, like
    /// `quote_id` — and, like it, it stays for the life of the document, so a
    /// bookkeeper can always see that a draft appeared because of a standing
    /// instruction rather than because a colleague typed it.
    pub schedule_id: Option<BillingScheduleId>,
    /// **Which** occurrence of that arrangement this document is for — the date
    /// the schedule was due on, not the day the run happened to notice. It is
    /// the pair `(schedule_id, schedule_due_date)` that the database holds
    /// unique, which is what makes a period impossible to bill twice.
    pub schedule_due_date: Option<Date>,
    /// The customer's own reference.
    pub reference: String,
    /// Free-text note.
    pub note: String,
    /// The exchange rate frozen on the document when it was issued (B1.21):
    /// what its amounts are restated into for the tenant's own books, at which
    /// rate, published on which day.
    ///
    /// `None` on a draft — the rate belongs to the moment the document became a
    /// document — and on a document issued before the snapshot existed in a
    /// currency other than the tenant's own, which is reported as unconverted
    /// rather than being assigned a rate nobody applied.
    pub fx: Option<FxSnapshot>,
    /// The user who created the document.
    pub created_by: String,
    /// Creation time.
    pub created_at: OffsetDateTime,
    /// Last modification time — moved by a header edit and by a line edit,
    /// since both change what the document says.
    pub updated_at: OffsetDateTime,
}

impl Invoice {
    /// Whether the document is **overdue** as of `today`: issued, still owed,
    /// and past the due date it was stamped with.
    ///
    /// Derived, never stored — a stored flag would be wrong every midnight,
    /// and the two facts it is derived from (status and due date) are frozen
    /// on the document already. It lives here rather than at the route edge so
    /// the list surface, the overdue view (B1.19) and the dunning drafts
    /// (B1.26) all answer from one definition.
    ///
    /// A `draft` has no due date, a `void` one is owed by nobody, and a `paid`
    /// one is settled; none of them is ever overdue. A **credit note** is not
    /// either, in any state: it is money owed to the customer, so a date passing
    /// on it makes nobody late.
    ///
    /// A **partially paid** document still is. It stays `issued` until the whole
    /// gross has arrived ([`crate::billing_payments`]), and it is overdue for
    /// the remainder — which is why partial payment is a fact about money and
    /// not a fifth status this predicate would have had to learn.
    pub fn is_overdue(&self, today: Date) -> bool {
        matches!(self.status, InvoiceStatus::Issued)
            && !self.is_credit_note
            && self.due_date.is_some_and(|due| due < today)
    }
}

/// An invoice as a list entry: the header and what it is worth, without the
/// lines. The totals are computed, never read from a column.
#[derive(Debug, Clone)]
pub struct InvoiceSummary {
    /// The header.
    pub invoice: Invoice,
    /// Net, VAT breakdown and gross, derived from the lines.
    pub totals: Totals,
    /// The sum of the payments recorded against this document, in cents
    /// ([`crate::billing_payments`]).
    pub paid_cents: i64,
}

impl InvoiceSummary {
    /// What is worth, what has arrived, and what is left.
    pub fn settlement(&self) -> Settlement {
        Settlement::of(self.totals.gross_cents, self.paid_cents)
    }

    /// The document's money in the tenant's accounting currency, or `None` when
    /// there is nothing to restate ([`crate::billing_fx::restated`]).
    pub fn base_totals(&self) -> Option<Totals> {
        restated(
            &self.invoice.currency,
            self.invoice.fx.as_ref(),
            &self.totals,
        )
    }
}

/// A whole document: header, lines in print order, and the totals derived
/// from those lines.
#[derive(Debug, Clone)]
pub struct InvoiceDocument {
    /// The header.
    pub invoice: Invoice,
    /// The lines, in print order.
    pub lines: Vec<Line>,
    /// Net, VAT breakdown and gross, derived from `lines`.
    pub totals: Totals,
    /// The sum of the payments recorded against this document, in cents
    /// ([`crate::billing_payments`]).
    pub paid_cents: i64,
}

impl InvoiceDocument {
    /// What it is worth, what has arrived, and what is left.
    pub fn settlement(&self) -> Settlement {
        Settlement::of(self.totals.gross_cents, self.paid_cents)
    }

    /// The document's money in the tenant's accounting currency, or `None` when
    /// there is nothing to restate ([`crate::billing_fx::restated`]) — the
    /// figure a foreign-currency invoice must print to state its VAT in the
    /// member state's own currency.
    pub fn base_totals(&self) -> Option<Totals> {
        restated(
            &self.invoice.currency,
            self.invoice.fx.as_ref(),
            &self.totals,
        )
    }
}

/// The header, validated and with the customer's defaults resolved.
/// A validated invoice header, ready to be written.
///
/// `pub(crate)` because the timesheet handoff ([`crate::time_invoice`]) resolves
/// a header and writes it inside its own transaction, rather than raising a
/// document first and hoping the rest of the call succeeds.
#[derive(Debug)]
pub(crate) struct NormalizedInvoice {
    pub(crate) customer_id: String,
    /// The currency the document is denominated in — the caller's, or the
    /// customer's own when they did not state one. Every amount that reaches
    /// the document has to be expressed in it.
    pub(crate) currency: String,
    pub(crate) payment_terms_days: i32,
    pub(crate) reference: String,
    pub(crate) note: String,
}

impl AccountStore {
    /// Resolves a header against **this tenant's** customer: the customer must
    /// exist under this handle, so a guessed id from another tenant is a
    /// `NotFound`, and it must be active — archiving a customer means "we no
    /// longer bill them", so raising a new document for one is a mistake
    /// worth reporting rather than obeying.
    async fn normalize_invoice(&self, input: &NewInvoice) -> Result<NormalizedInvoice> {
        let mut conn = self.pool.acquire().await.map_err(StoreError::Db)?;
        self.normalize_invoice_in(&mut conn, input).await
    }

    /// [`AccountStore::normalize_invoice`] on a caller's connection, so a
    /// document raised inside a transaction resolves its customer under the same
    /// transaction that writes it.
    pub(crate) async fn normalize_invoice_in(
        &self,
        conn: &mut PgConnection,
        input: &NewInvoice,
    ) -> Result<NormalizedInvoice> {
        let customer = customer_read(&mut *conn, self.tenant.as_str(), &input.customer_id)
            .await?
            .ok_or(StoreError::NotFound)?;
        if customer.is_archived() {
            return Err(StoreError::Validation(
                "the customer is archived; restore it before billing it again".to_owned(),
            ));
        }
        let resolved_currency = match input.currency.as_deref() {
            Some(code) => currency(code)?,
            None => customer.currency,
        };
        let resolved_terms = match input.payment_terms_days {
            Some(days) => payment_terms_days(days)?,
            None => customer.payment_terms_days,
        };
        Ok(NormalizedInvoice {
            customer_id: customer.id.as_str().to_owned(),
            currency: resolved_currency,
            payment_terms_days: resolved_terms,
            reference: bounded("reference", &input.reference, INVOICE_REFERENCE_MAX_CHARS)?,
            note: bounded("note", &input.note, INVOICE_NOTE_MAX_CHARS)?,
        })
    }

    /// Takes the document's row lock inside `tx` and returns the few stored
    /// facts a write has to decide against, so a caller can check whether it
    /// may write and then write it without any other transaction slipping in
    /// between. Two writers to one document serialise here; a writer that
    /// arrives after an issue sees `issued`.
    ///
    /// It reads more than the status because two of those decisions are about
    /// what the document *is* rather than where it is: a credit note may not be
    /// moved to another customer or currency, and it may not itself be
    /// credited.
    ///
    /// # Errors
    /// [`StoreError::NotFound`] when the id is absent **or another tenant's**;
    /// [`StoreError::Db`] on failure or on a status the code does not know.
    async fn lock_invoice(
        &self,
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
        id: &BillingInvoiceId,
    ) -> Result<LockedInvoice> {
        let row: Option<LockedRow> = sqlx::query_as(
            "SELECT status, is_credit_note, credits_invoice_id, customer_id, currency, \
                 payment_terms_days, reference \
             FROM billing_invoices WHERE tenant_id = $1 AND id = $2 FOR UPDATE",
        )
        .bind(self.tenant.as_str())
        .bind(id.as_str())
        .fetch_optional(&mut **tx)
        .await
        .map_err(StoreError::Db)?;
        let row = row.ok_or(StoreError::NotFound)?;
        Ok(LockedInvoice {
            status: parse_stored_status(&row.status)?,
            is_credit_note: row.is_credit_note,
            credits_invoice_id: row.credits_invoice_id,
            customer_id: row.customer_id,
            currency: row.currency,
            payment_terms_days: row.payment_terms_days,
            reference: row.reference,
        })
    }

    /// The status of one of this tenant's documents, without taking a lock —
    /// the cheap pre-check that lets a write refuse a frozen document before
    /// it does any other work. It is never the authority: every write re-reads
    /// the status under [`AccountStore::lock_invoice_status`] before writing.
    ///
    /// # Errors
    /// [`StoreError::NotFound`] when the id is absent or another tenant's;
    /// [`StoreError::Db`] on failure.
    async fn invoice_status(&self, id: &BillingInvoiceId) -> Result<InvoiceStatus> {
        let stored: Option<String> = sqlx::query_scalar(
            "SELECT status FROM billing_invoices WHERE tenant_id = $1 AND id = $2",
        )
        .bind(self.tenant.as_str())
        .bind(id.as_str())
        .fetch_optional(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        parse_stored_status(&stored.ok_or(StoreError::NotFound)?)
    }

    /// Creates a **draft** invoice with no lines — the state a new document
    /// starts in. It carries no number and no dates by construction; only
    /// issuing (B1.08) assigns those.
    ///
    /// # Errors
    /// [`StoreError::NotFound`] when the customer is not this tenant's;
    /// [`StoreError::Validation`] when the customer is archived or a header
    /// field breaks its rule; [`StoreError::Db`] on failure.
    pub async fn create_billing_invoice(&self, input: &NewInvoice) -> Result<BillingInvoiceId> {
        let mut conn = self.pool.acquire().await.map_err(StoreError::Db)?;
        let header = self.normalize_invoice_in(&mut conn, input).await?;
        self.insert_draft_invoice(&mut conn, &header).await
    }

    /// Writes a draft invoice row on the caller's connection and answers its id
    /// — **the one place a draft invoice is inserted**, lifted out so the
    /// timesheet handoff ([`crate::time_invoice`]) can raise the document, write
    /// its lines and stamp the hours it carries in a single transaction.
    ///
    /// # Errors
    /// [`StoreError::Db`] on failure.
    pub(crate) async fn insert_draft_invoice(
        &self,
        conn: &mut PgConnection,
        header: &NormalizedInvoice,
    ) -> Result<BillingInvoiceId> {
        let id = BillingInvoiceId::generate();
        sqlx::query(
            "INSERT INTO billing_invoices (tenant_id, id, customer_id, status, currency, \
                 payment_terms_days, reference, note, created_by) \
             VALUES ($1, $2, $3, 'draft', $4, $5, $6, $7, $8)",
        )
        .bind(self.tenant.as_str())
        .bind(id.as_str())
        .bind(&header.customer_id)
        .bind(&header.currency)
        .bind(header.payment_terms_days)
        .bind(&header.reference)
        .bind(&header.note)
        .bind(self.user.as_str())
        .execute(conn)
        .await
        .map_err(StoreError::Db)?;
        Ok(id)
    }

    /// The tenant's invoices, newest first, each with its computed totals and
    /// the money received against it. `status` filters; `None` lists
    /// everything.
    ///
    /// The lines and the payments of every listed document are fetched in one
    /// further statement each, not one per document.
    ///
    /// # Errors
    /// [`StoreError::Db`] on failure.
    pub async fn billing_invoices(
        &self,
        status: Option<InvoiceStatus>,
    ) -> Result<Vec<InvoiceSummary>> {
        self.list_billing_invoices(status, false).await
    }

    /// The tenant's **overdue** invoices, newest first: issued, past the due
    /// date they were stamped with, and not settled.
    ///
    /// It is the same list read behind one predicate, so the `overdue` flag a
    /// caller sees on an entry and its presence in this list can never
    /// disagree ([`Invoice::is_overdue`]). Judged against the **database's**
    /// date, inside the same statement, never a date a caller sends.
    ///
    /// "Not settled" is the status column doing its one job: a document stays
    /// `issued` until the whole gross has arrived, so a partially paid one is
    /// here — overdue for the remainder — and a fully paid one is not. Credit
    /// notes are excluded: money owed to the customer makes nobody late.
    ///
    /// # Errors
    /// [`StoreError::Db`] on failure.
    pub async fn billing_overdue_invoices(&self) -> Result<Vec<InvoiceSummary>> {
        self.list_billing_invoices(None, true).await
    }

    /// The one list read behind both surfaces above: headers, then every
    /// listed document's lines, then every listed document's payments — three
    /// statements whatever the length of the list.
    async fn list_billing_invoices(
        &self,
        status: Option<InvoiceStatus>,
        overdue_only: bool,
    ) -> Result<Vec<InvoiceSummary>> {
        let status = status.map(InvoiceStatus::as_str);
        // Both branches state `$2` so the two reads below take the same
        // parameters whichever surface asked; the overdue view simply binds no
        // status of its own.
        let scope = if overdue_only {
            "($2::text IS NULL OR status = $2) AND status = 'issued' \
             AND is_credit_note = false AND due_date < CURRENT_DATE"
        } else {
            "($2::text IS NULL OR status = $2)"
        };
        let rows = sqlx::query_as::<_, InvoiceRow>(&format!(
            "SELECT {INVOICE_COLS} FROM billing_invoices \
             WHERE tenant_id = $1 AND {scope} \
             ORDER BY created_at DESC, id"
        ))
        .bind(self.tenant.as_str())
        .bind(status)
        .fetch_all(&self.pool)
        .await
        .map_err(StoreError::Db)?;

        let figures = sqlx::query_as::<_, FiguresRow>(&format!(
            "SELECT invoice_id AS doc_id, qty_milli, unit_price_cents, vat_rate_bp \
             FROM billing_invoice_lines \
             WHERE tenant_id = $1 AND invoice_id IN ( \
                 SELECT id FROM billing_invoices WHERE tenant_id = $1 AND {scope})"
        ))
        .bind(self.tenant.as_str())
        .bind(status)
        .fetch_all(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        let mut by_invoice = group_figures(figures);

        let paid: Vec<(String, Option<i64>)> = sqlx::query_as(&format!(
            "SELECT invoice_id, sum(amount_cents)::bigint FROM billing_payments \
             WHERE tenant_id = $1 AND invoice_id IN ( \
                 SELECT id FROM billing_invoices WHERE tenant_id = $1 AND {scope}) \
             GROUP BY invoice_id"
        ))
        .bind(self.tenant.as_str())
        .bind(status)
        .fetch_all(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        let mut by_paid: HashMap<String, i64> = paid
            .into_iter()
            .map(|(id, sum)| (id, sum.unwrap_or(0)))
            .collect();

        rows.into_iter()
            .map(|row| {
                let lines = by_invoice.remove(&row.id).unwrap_or_default();
                let paid_cents = by_paid.remove(&row.id).unwrap_or(0);
                Ok(InvoiceSummary {
                    invoice: row.into_invoice()?,
                    totals: totals(&lines),
                    paid_cents,
                })
            })
            .collect()
    }

    /// One document of the tenant with its lines and totals, or `None` —
    /// including when the id belongs to another tenant (indistinguishable by
    /// design).
    ///
    /// # Errors
    /// [`StoreError::Db`] on failure.
    pub async fn billing_invoice(&self, id: &BillingInvoiceId) -> Result<Option<InvoiceDocument>> {
        let mut conn = self.pool.acquire().await.map_err(StoreError::Db)?;
        self.billing_invoice_with(&mut conn, id).await
    }

    /// [`AccountStore::billing_invoice`], inside a transaction the caller owns.
    ///
    /// The booking layer has to read the document **as the transaction sees
    /// it** — an issue that has just stamped the number and frozen the rate
    /// books that document, not the draft the pool would still show
    /// ([`crate::fin_booking`]).
    ///
    /// # Errors
    /// [`StoreError::Db`] on failure.
    pub(crate) async fn billing_invoice_in(
        &self,
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
        id: &BillingInvoiceId,
    ) -> Result<Option<InvoiceDocument>> {
        self.billing_invoice_with(tx, id).await
    }

    /// The one loading path behind the two doors above: the row, its lines, and
    /// the money received, read through whichever connection the caller holds.
    async fn billing_invoice_with(
        &self,
        conn: &mut sqlx::PgConnection,
        id: &BillingInvoiceId,
    ) -> Result<Option<InvoiceDocument>> {
        let Some(row) = sqlx::query_as::<_, InvoiceRow>(&format!(
            "SELECT {INVOICE_COLS} FROM billing_invoices WHERE tenant_id = $1 AND id = $2"
        ))
        .bind(self.tenant.as_str())
        .bind(id.as_str())
        .fetch_optional(&mut *conn)
        .await
        .map_err(StoreError::Db)?
        else {
            return Ok(None);
        };
        let lines = INVOICE_LINES
            .read(&mut *conn, self.tenant.as_str(), id.as_str())
            .await?;
        let figures: Vec<LineFigures> = lines.iter().map(Line::figures).collect();
        // The money received, read here rather than by a second call: every
        // reader of a document (the screen, the print view, the covering mail)
        // then sees the same settlement, and none of them has to know that
        // payments are a separate table.
        let paid_cents: Option<i64> = sqlx::query_scalar(
            "SELECT sum(amount_cents)::bigint FROM billing_payments \
             WHERE tenant_id = $1 AND invoice_id = $2",
        )
        .bind(self.tenant.as_str())
        .bind(id.as_str())
        .fetch_one(&mut *conn)
        .await
        .map_err(StoreError::Db)?;
        Ok(Some(InvoiceDocument {
            invoice: row.into_invoice()?,
            lines,
            totals: totals(&figures),
            paid_cents: paid_cents.unwrap_or(0),
        }))
    }

    /// The id of the tenant's invoice **numbered** `number`, or `None`.
    ///
    /// The way a person names a document is its number ("INV-2026-00042"), and
    /// that is all the billing agent (B1.25) is ever given: a model is told a
    /// number the user said, never an opaque id, and this is where such a name
    /// becomes something the store can act on. It answers an id rather than a
    /// document so the caller then reads it through the ordinary
    /// [`AccountStore::billing_invoice`] door — one loading path, not two.
    ///
    /// Matching is case-insensitive and ignores surrounding blanks (a number
    /// arrives copied out of an email as often as typed), but is otherwise
    /// exact: a prefix is not a document. Only **issued** documents have a
    /// number at all, so a draft is unreachable by this route by construction.
    /// Another tenant's number is `None`, exactly as their id is.
    ///
    /// # Errors
    /// [`StoreError::Db`] on failure.
    pub async fn billing_invoice_id_by_number(
        &self,
        number: &str,
    ) -> Result<Option<BillingInvoiceId>> {
        let wanted = number.trim();
        if wanted.is_empty() {
            return Ok(None);
        }
        let id: Option<String> = sqlx::query_scalar(
            "SELECT id FROM billing_invoices \
             WHERE tenant_id = $1 AND upper(number) = upper($2)",
        )
        .bind(self.tenant.as_str())
        .bind(wanted)
        .fetch_optional(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        Ok(id.map(BillingInvoiceId::new))
    }

    /// The tenant's invoices carrying any of `numbers`, with their totals and
    /// what has been received against them.
    ///
    /// The batch form of [`AccountStore::billing_invoice_id_by_number`], and it
    /// exists for one caller: reconciliation reads a statement's remittances,
    /// finds the numbers a payer quoted ([`crate::bank_match`]) and needs every
    /// one of those documents *with its settlement* before it can say which
    /// lines are exact matches. Asking per number would be three statements per
    /// line of a bank file.
    ///
    /// Matching is case-insensitive, like the single lookup, and only issued
    /// documents carry a number at all. An empty list reads nothing rather than
    /// everything.
    ///
    /// # Errors
    /// [`StoreError::Db`] on failure.
    pub(crate) async fn billing_invoices_by_numbers(
        &self,
        numbers: &[String],
    ) -> Result<Vec<InvoiceSummary>> {
        if numbers.is_empty() {
            return Ok(Vec::new());
        }
        let wanted: Vec<String> = numbers
            .iter()
            .map(|number| number.trim().to_uppercase())
            .collect();
        // One scope in all three statements, so a document cannot appear in one
        // and be missing from another.
        let scope = "upper(number) = ANY($2::text[])";
        let rows = sqlx::query_as::<_, InvoiceRow>(&format!(
            "SELECT {INVOICE_COLS} FROM billing_invoices \
             WHERE tenant_id = $1 AND {scope} \
             ORDER BY created_at DESC, id"
        ))
        .bind(self.tenant.as_str())
        .bind(&wanted)
        .fetch_all(&self.pool)
        .await
        .map_err(StoreError::Db)?;

        let figures = sqlx::query_as::<_, FiguresRow>(&format!(
            "SELECT invoice_id AS doc_id, qty_milli, unit_price_cents, vat_rate_bp \
             FROM billing_invoice_lines \
             WHERE tenant_id = $1 AND invoice_id IN ( \
                 SELECT id FROM billing_invoices WHERE tenant_id = $1 AND {scope})"
        ))
        .bind(self.tenant.as_str())
        .bind(&wanted)
        .fetch_all(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        let mut by_invoice = group_figures(figures);

        let paid: Vec<(String, Option<i64>)> = sqlx::query_as(&format!(
            "SELECT invoice_id, sum(amount_cents)::bigint FROM billing_payments \
             WHERE tenant_id = $1 AND invoice_id IN ( \
                 SELECT id FROM billing_invoices WHERE tenant_id = $1 AND {scope}) \
             GROUP BY invoice_id"
        ))
        .bind(self.tenant.as_str())
        .bind(&wanted)
        .fetch_all(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        let mut by_paid: HashMap<String, i64> = paid
            .into_iter()
            .map(|(id, sum)| (id, sum.unwrap_or(0)))
            .collect();

        rows.into_iter()
            .map(|row| {
                let lines = by_invoice.remove(&row.id).unwrap_or_default();
                let paid_cents = by_paid.remove(&row.id).unwrap_or(0);
                Ok(InvoiceSummary {
                    invoice: row.into_invoice()?,
                    totals: totals(&lines),
                    paid_cents,
                })
            })
            .collect()
    }

    /// Replaces the writable header of a **draft** invoice: customer,
    /// currency, terms, reference and note. Status, number and dates are not
    /// writable here — they move only through the lifecycle actions.
    ///
    /// The document's status is checked before the header is even validated,
    /// so a frozen document is told it is frozen rather than being handed a
    /// complaint about a field it was never going to accept; it is then
    /// re-checked under the row lock that the write itself takes.
    ///
    /// A **credit note** additionally keeps the customer and the currency it
    /// was raised with: it exists to reverse one specific document, and a
    /// credit note billed to somebody else — or denominated in another currency
    /// — reverses nothing. Everything else about it (terms, reference, note,
    /// and the lines, so a partial credit is a matter of editing them) stays
    /// freely editable while it is a draft.
    ///
    /// # Errors
    /// [`StoreError::NotFound`] when the invoice or the customer is not this
    /// tenant's; [`StoreError::Conflict`] when the invoice is no longer a
    /// draft; [`StoreError::Validation`] as for create, or when a credit note
    /// is moved off its original's customer or currency; [`StoreError::Db`]
    /// on failure.
    pub async fn update_billing_invoice(
        &self,
        id: &BillingInvoiceId,
        input: &NewInvoice,
    ) -> Result<()> {
        self.invoice_status(id).await?.ensure_editable()?;
        let header = self.normalize_invoice(input).await?;

        let mut tx = self.pool.begin().await.map_err(StoreError::Db)?;
        // Authoritative: the state that matters is the one under the lock the
        // UPDATE below writes through, not the one read a moment ago.
        // Dropping the transaction on any error rolls it back untouched.
        let locked = self.lock_invoice(&mut tx, id).await?;
        locked.status.ensure_editable()?;
        if locked.is_credit_note
            && (header.customer_id != locked.customer_id || header.currency != locked.currency)
        {
            return Err(StoreError::Validation(
                "a credit note stays on the customer and currency of the invoice it credits"
                    .to_owned(),
            ));
        }
        sqlx::query(
            "UPDATE billing_invoices SET customer_id = $3, currency = $4, \
                 payment_terms_days = $5, reference = $6, note = $7, updated_at = now() \
             WHERE tenant_id = $1 AND id = $2",
        )
        .bind(self.tenant.as_str())
        .bind(id.as_str())
        .bind(&header.customer_id)
        .bind(&header.currency)
        .bind(header.payment_terms_days)
        .bind(&header.reference)
        .bind(&header.note)
        .execute(&mut *tx)
        .await
        .map_err(StoreError::Db)?;
        tx.commit().await.map_err(StoreError::Db)?;
        Ok(())
    }

    /// Replaces the whole line set of a **draft** invoice, in the caller's
    /// order, in one transaction: either the document reads exactly as the
    /// caller sent it or it is untouched. Line positions are assigned 0-based
    /// from that order, so what was sent is what prints.
    ///
    /// Every line is validated **before** anything is written, so a document
    /// is never left half-replaced by a bad line at the end — and the
    /// draft-only guard runs before even that, under the same lock the
    /// replacement writes through, so a set cannot land on a document that was
    /// issued while it was being composed.
    ///
    /// # Errors
    /// [`StoreError::NotFound`] when the invoice is not this tenant's;
    /// [`StoreError::Conflict`] when the invoice is no longer a draft;
    /// [`StoreError::Validation`] when the set is too long or a line breaks a
    /// field rule (the message names the line's position);
    /// [`StoreError::Db`] on failure.
    pub async fn set_billing_invoice_lines(
        &self,
        id: &BillingInvoiceId,
        lines: &[NewLine],
    ) -> Result<()> {
        let mut tx = self.pool.begin().await.map_err(StoreError::Db)?;
        // Lock the document for the whole replacement: two editors saving at
        // once serialise here instead of interleaving their line sets, and an
        // issue that raced this save either lost (and sees these lines) or won
        // (and this save is refused). Dropping the transaction on any error
        // below rolls it back; nothing was written.
        self.lock_invoice(&mut tx, id)
            .await?
            .status
            .ensure_editable()?;
        INVOICE_LINES
            .replace(&mut tx, self.tenant.as_str(), id.as_str(), lines)
            .await?;

        sqlx::query(
            "UPDATE billing_invoices SET updated_at = now() WHERE tenant_id = $1 AND id = $2",
        )
        .bind(self.tenant.as_str())
        .bind(id.as_str())
        .execute(&mut *tx)
        .await
        .map_err(StoreError::Db)?;

        tx.commit().await.map_err(StoreError::Db)?;
        Ok(())
    }

    /// Deletes a **draft** invoice and, by cascade, its lines. This is the
    /// only document that is ever removed: a draft never consumed a number, so
    /// abandoning it leaves no hole in the sequence and no record anyone is
    /// entitled to. An issued document is voided instead (B1.08+), keeping its
    /// number and its content readable.
    ///
    /// # Errors
    /// [`StoreError::NotFound`] when the invoice is absent or another
    /// tenant's; [`StoreError::Conflict`] when it is no longer a draft;
    /// [`StoreError::Db`] on failure.
    pub async fn delete_billing_invoice(&self, id: &BillingInvoiceId) -> Result<()> {
        let mut tx = self.pool.begin().await.map_err(StoreError::Db)?;
        self.lock_invoice(&mut tx, id)
            .await?
            .status
            .ensure_editable()?;
        release_billed_hours(&mut tx, self.tenant.as_str(), id).await?;
        sqlx::query("DELETE FROM billing_invoices WHERE tenant_id = $1 AND id = $2")
            .bind(self.tenant.as_str())
            .bind(id.as_str())
            .execute(&mut *tx)
            .await
            .map_err(StoreError::Db)?;
        tx.commit().await.map_err(StoreError::Db)?;
        Ok(())
    }

    /// Issues a **draft** invoice: draws the next number from this tenant's
    /// gapless series, stamps the issue and due dates, and freezes the
    /// document. This is the only transition that assigns a number, and it is
    /// irreversible — an issued document is corrected by voiding it or by
    /// crediting it (B1.09), never by being edited back into a draft.
    ///
    /// Everything happens in **one transaction**: the document's row lock is
    /// taken first (so a save that raced this issue is refused rather than
    /// applied afterwards), then the counter's row lock. Both locks are taken
    /// in that order by every issue, so concurrent issues queue instead of
    /// deadlocking, and if anything below fails the number is given back —
    /// which is the entire reason the counter is a row and not a Postgres
    /// `SEQUENCE` (see [`crate::billing_sequence`]).
    ///
    /// The issue date is **today according to the database**, read inside the
    /// same transaction rather than taken from the caller: the series' numbers
    /// must ascend together with their dates, and a caller-supplied date is
    /// how that stops being true. The due date is that day plus the payment
    /// terms already snapshotted on the document.
    ///
    /// An invoice with no lines is refused. It would be worth nothing, and
    /// issuing it would spend a number of a legally unbroken series on a
    /// document that says nothing — a mistake worth reporting rather than
    /// obeying.
    ///
    /// Issuing also **freezes the exchange rate** the document's money is
    /// restated at for the tenant's own books (`issue_fx_snapshot`). A
    /// document raised in the accounting currency takes the identity rate; a
    /// foreign-currency one is refused when no reference rate has been imported
    /// for its currency, because an invoice that cannot state its VAT in the
    /// member state's currency is legally incomplete.
    ///
    /// Issuing also **books the document** ([`AccountStore::book_issue_in`],
    /// B7.01): the entry that puts the receivable, the revenue and the output
    /// tax into the journal is written inside this same transaction, so a
    /// document and its books can never disagree about whether it happened. A
    /// booking refusal — a chart with no account for a role the rule needs, an
    /// issue date inside a closed period — therefore fails the issue whole,
    /// and the number is given back.
    ///
    /// # Errors
    /// [`StoreError::NotFound`] when the invoice is absent or another
    /// tenant's; [`StoreError::Conflict`] when it is not a draft, or when its
    /// issue date falls in a closed period (B4.10);
    /// [`StoreError::Validation`] when it has no lines, when no exchange rate
    /// is available for its currency, or when the chart of accounts cannot
    /// answer a role the posting rule needs; [`StoreError::Db`] on failure.
    pub async fn issue_billing_invoice(&self, id: &BillingInvoiceId) -> Result<InvoiceDocument> {
        let mut tx = self.pool.begin().await.map_err(StoreError::Db)?;
        // The lock also hands back the terms this document was raised with —
        // never the customer's current ones — which is what the due date below
        // is derived from.
        let locked = self.lock_invoice(&mut tx, id).await?;
        locked.status.ensure_issuable()?;
        let terms = locked.payment_terms_days;

        // Read under the same lock: whether the document says anything at all.
        let lines: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM billing_invoice_lines WHERE tenant_id = $1 AND invoice_id = $2",
        )
        .bind(self.tenant.as_str())
        .bind(id.as_str())
        .fetch_one(&mut *tx)
        .await
        .map_err(StoreError::Db)?;
        if lines == 0 {
            return Err(StoreError::Validation(
                "an invoice with no lines cannot be issued; add a line first".to_owned(),
            ));
        }

        // One clock for the whole transaction, and the same clock the row's
        // own timestamps use.
        let today: Date = sqlx::query_scalar("SELECT CURRENT_DATE")
            .fetch_one(&mut *tx)
            .await
            .map_err(StoreError::Db)?;
        let due = today
            .checked_add(Duration::days(i64::from(terms)))
            .ok_or_else(|| {
                StoreError::Validation(
                    "the payment terms put the due date outside the supported range".to_owned(),
                )
            })?;
        // The rate is frozen in the same step as the number and the dates: EU
        // VAT Directive art. 91 fixes it at the tax point, so it is a fact about
        // the document rather than something a later read re-derives. A credit
        // note inherits its original's rate (see `issue_fx_snapshot`).
        let base = base_currency_in(&mut tx, self.tenant.as_str()).await?;
        let fx = self
            .issue_fx_snapshot(&mut tx, &locked, &base, today)
            .await?;

        let drawn = draw_next(
            &mut tx,
            self.tenant.as_str(),
            INVOICE_SEQUENCE_KIND,
            today.year(),
        )
        .await?;
        let number = document_number(INVOICE_NUMBER_PREFIX, today.year(), drawn);

        sqlx::query(
            "UPDATE billing_invoices \
                SET status = 'issued', number = $3, issue_date = $4, due_date = $5, \
                    fx_base_currency = $6, fx_rate_micro = $7, fx_rate_date = $8, \
                    updated_at = now() \
             WHERE tenant_id = $1 AND id = $2",
        )
        .bind(self.tenant.as_str())
        .bind(id.as_str())
        .bind(&number)
        .bind(today)
        .bind(due)
        .bind(&fx.base_currency)
        .bind(fx.rate_micro)
        .bind(fx.rate_date)
        .execute(&mut *tx)
        .await
        .map_err(StoreError::Db)?;

        // The books learn about the document in the same act that makes it
        // real (`docs/design/finance.md`, "the posting happens inside the
        // document's own transaction"; B7.01). The document is re-read under
        // this transaction so the entry books the number, dates and rate just
        // stamped — and if the booking refuses (a chart missing a role, a
        // closed period), the whole issue rolls back and the drawn number
        // returns to the sequence.
        let document = self
            .billing_invoice_in(&mut tx, id)
            .await?
            .ok_or(StoreError::NotFound)?;
        self.book_issue_in(&mut tx, &document, &base).await?;
        tx.commit().await.map_err(StoreError::Db)?;

        Ok(document)
    }

    /// The exchange-rate snapshot the document being issued is frozen with.
    ///
    /// A document raised in the tenant's own accounting currency — nearly all of
    /// them — takes the identity rate and needs no rate table at all. A
    /// foreign-currency one takes the last rate published at or before today
    /// ([`crate::billing_fx_rates::snapshot_at`]), and is **refused** when there
    /// is none: without a rate the document cannot state its VAT in the member
    /// state's currency (art. 230), so issuing it would produce an invoice that
    /// is legally incomplete.
    ///
    /// **A credit note inherits its original's rate**, not today's. The
    /// correction relates to the supply the original invoiced, so both documents
    /// have to convert identically or the pair would not sum to zero in the
    /// books — the same reason the credit note mirrors the original's lines
    /// exactly. Only when the original carries no snapshot (a document issued
    /// before B1.21) does the credit note resolve its own; that is flagged in
    /// `docs/design/billing.md` for human review.
    async fn issue_fx_snapshot(
        &self,
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
        locked: &LockedInvoice,
        base_currency: &str,
        today: Date,
    ) -> Result<FxSnapshot> {
        if let Some(original_id) = locked
            .credits_invoice_id
            .as_deref()
            .filter(|_| locked.is_credit_note)
        {
            let inherited: Option<(Option<String>, Option<i64>, Option<Date>)> = sqlx::query_as(
                "SELECT fx_base_currency, fx_rate_micro, fx_rate_date FROM billing_invoices \
                 WHERE tenant_id = $1 AND id = $2",
            )
            .bind(self.tenant.as_str())
            .bind(original_id)
            .fetch_optional(&mut **tx)
            .await
            .map_err(StoreError::Db)?;
            if let Some((Some(base_currency), Some(rate_micro), Some(rate_date))) = inherited {
                return Ok(FxSnapshot {
                    base_currency,
                    rate_micro,
                    rate_date,
                });
            }
        }
        snapshot_at(
            tx,
            self.tenant.as_str(),
            base_currency,
            &locked.currency,
            today,
        )
        .await
    }

    /// Voids an **issued** invoice: it keeps its number, its dates and its
    /// lines, and stops being owed. Nothing is deleted — the series stays
    /// gapless precisely because a cancelled document remains in it, readable,
    /// marked as cancelled.
    ///
    /// Voiding suits a document that never left the building. One the customer
    /// already holds is corrected with a credit note (B1.09) instead, so that
    /// both parties' copies still reconcile; the store cannot know which case
    /// it is looking at, so it allows the transition and says so here.
    ///
    /// **Except when money has arrived.** A document with any recorded payment
    /// is refused: cancelling it would leave received money attached to a
    /// document that says nothing is owed, which is a hole in the ledger rather
    /// than a correction. That case is a credit note too — the payment then
    /// settles a debt the credit note has reduced, and both movements stay
    /// visible. (A fully paid document is already refused by
    /// [`InvoiceStatus::ensure_voidable`]; this catches the partially paid one,
    /// which is still `issued`.)
    ///
    /// # Errors
    /// [`StoreError::NotFound`] when the invoice is absent or another
    /// tenant's; [`StoreError::Conflict`] when it is not issued (a draft is
    /// deleted, a paid document is credited), when payments have been
    /// recorded against it, or when its issue entry lies in a closed period
    /// (the reversal that takes it back cannot be written there);
    /// [`StoreError::Db`] on failure.
    pub async fn void_billing_invoice(&self, id: &BillingInvoiceId) -> Result<InvoiceDocument> {
        let mut tx = self.pool.begin().await.map_err(StoreError::Db)?;
        self.lock_invoice(&mut tx, id)
            .await?
            .status
            .ensure_voidable()?;
        // Read under the same lock as the write, so a payment that raced this
        // void either lands first (and the void is refused) or waits.
        let payments: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM billing_payments WHERE tenant_id = $1 AND invoice_id = $2",
        )
        .bind(self.tenant.as_str())
        .bind(id.as_str())
        .fetch_one(&mut *tx)
        .await
        .map_err(StoreError::Db)?;
        if payments > 0 {
            return Err(StoreError::Conflict(
                "money has been received against this invoice; correct it with a credit note \
                 instead of voiding it"
                    .to_owned(),
            ));
        }
        release_billed_hours(&mut tx, self.tenant.as_str(), id).await?;
        sqlx::query(
            "UPDATE billing_invoices SET status = 'void', updated_at = now() \
             WHERE tenant_id = $1 AND id = $2",
        )
        .bind(self.tenant.as_str())
        .bind(id.as_str())
        .execute(&mut *tx)
        .await
        .map_err(StoreError::Db)?;

        // A booked document that stops being owed is corrected in the books,
        // never erased from them: the issue entry stays, and a reversal dated
        // the document's own issue date takes it back (`fin_rules`' words — "a
        // void one is booked by its issue entry and reversed by its void
        // entry"; B7.01) — so the period the document was booked into nets to
        // zero, as befits a document that never left the building. When that
        // period has since been closed the reversal is refused and the void
        // fails whole, which is the honest answer: a document a filed report
        // counted is corrected with a credit note. A document from before the
        // books opened has no entry and voids as it always did.
        if let Some(entry_id) = self.fin_invoice_entry_in(&mut tx, id).await? {
            let issued = self
                .fin_journal_entry(&entry_id)
                .await?
                .ok_or(StoreError::NotFound)?;
            let reversal = reversal_entry(
                &issued,
                Some(EntrySource {
                    kind: SourceKind::Invoice,
                    id: id.as_str().to_owned(),
                    event: SourceEvent::Void,
                }),
            );
            self.post_fin_entry_in(&mut tx, &reversal).await?;
        }
        tx.commit().await.map_err(StoreError::Db)?;

        self.billing_invoice(id).await?.ok_or(StoreError::NotFound)
    }

    /// Raises a **draft credit note** that mirrors an issued document: the same
    /// customer, currency, terms and customer reference, and a copy of every
    /// line with its quantity negated, in the original's order.
    ///
    /// It is a draft, not a finished document, and that is the point: the
    /// mirror is the starting position, and a **partial** credit is a matter of
    /// editing its lines before issuing it. Issuing it goes through the ordinary
    /// [`AccountStore::issue_billing_invoice`], so it draws from the **same**
    /// per-tenant series as the invoice it credits — an unbroken ledger is one
    /// series, not two interleaved ones — and it is frozen by the same rules.
    ///
    /// A full mirror is exactly worth the negative of its original, down to the
    /// per-rate VAT breakdown: [`crate::billing_totals`] rounds half away from
    /// zero precisely so that `totals(−lines) == −totals(lines)`, and the two
    /// documents therefore sum to zero with no residual cent.
    ///
    /// The original must be a document the customer actually holds — `issued`
    /// or `paid` (see [`InvoiceStatus::ensure_creditable`]) — and it must not
    /// itself be a credit note: crediting a credit note is an invoice, and
    /// raising one as a "credit" would put a positive document in the credit
    /// chain, where every ledger walk expects the opposite sign.
    ///
    /// The customer is copied rather than re-resolved, so an **archived**
    /// customer can still be credited. Archiving means "we raise no new
    /// business for them"; correcting a document already in their hands is not
    /// new business, and refusing it would leave a wrong invoice standing
    /// forever.
    ///
    /// # Errors
    /// [`StoreError::NotFound`] when the original is absent or another
    /// tenant's; [`StoreError::Conflict`] when it is a draft, void, or itself
    /// a credit note; [`StoreError::Db`] on failure.
    pub async fn create_billing_credit_note(
        &self,
        original_id: &BillingInvoiceId,
    ) -> Result<BillingInvoiceId> {
        let mut tx = self.pool.begin().await.map_err(StoreError::Db)?;
        // The original's lock is held for the whole copy: a void racing this
        // call either lands first (and the credit is refused) or waits.
        let original = self.lock_invoice(&mut tx, original_id).await?;
        // What the document *is* outranks where it is in its life: a credit
        // note is never creditable, in any state, so a caller gets that one
        // stable answer rather than a state complaint that would change to a
        // different refusal once the same document is issued.
        if original.is_credit_note {
            return Err(StoreError::Conflict(
                "a credit note cannot itself be credited; raise an invoice instead".to_owned(),
            ));
        }
        original.status.ensure_creditable()?;

        let id = BillingInvoiceId::generate();
        sqlx::query(
            "INSERT INTO billing_invoices (tenant_id, id, customer_id, status, currency, \
                 payment_terms_days, is_credit_note, credits_invoice_id, reference, note, \
                 created_by) \
             VALUES ($1, $2, $3, 'draft', $4, $5, true, $6, $7, '', $8)",
        )
        .bind(self.tenant.as_str())
        .bind(id.as_str())
        .bind(&original.customer_id)
        .bind(&original.currency)
        .bind(original.payment_terms_days)
        .bind(original_id.as_str())
        .bind(&original.reference)
        .bind(self.user.as_str())
        .execute(&mut *tx)
        .await
        .map_err(StoreError::Db)?;

        // The mirror: the same descriptions, units, prices and rates, in the
        // same print order, with the quantity negated. Read inside the same
        // transaction, under the original's lock. The quantity bound is
        // symmetric ([`crate::billing_line::QTY_MAX_MILLI`]), so a stored
        // quantity always has a storable negation, and every line gets an id of
        // its own like any other written line — a credit note's lines are
        // ordinary lines, not shadows of the original's.
        let source = INVOICE_LINES
            .read(&mut *tx, self.tenant.as_str(), original_id.as_str())
            .await?;

        for line in &source {
            INVOICE_LINES
                .write(
                    &mut tx,
                    self.tenant.as_str(),
                    id.as_str(),
                    line.line_order,
                    &line.negated(),
                )
                .await?;
        }

        tx.commit().await.map_err(StoreError::Db)?;
        Ok(id)
    }

    /// The credit notes raised against one of this tenant's documents, newest
    /// first, each with its computed totals — the read side of the credit
    /// relation, and the ledger of a corrected invoice: the original's gross
    /// plus the gross of its *issued* credit notes is what is actually owed.
    ///
    /// Drafts are included, because a credit note being prepared is a fact the
    /// invoice's screen must show; the caller distinguishes them by status
    /// rather than by their absence.
    ///
    /// An id that is absent or another tenant's yields an empty list, like
    /// every other list read here — never an existence oracle.
    ///
    /// # Errors
    /// [`StoreError::Db`] on failure.
    pub async fn billing_credit_notes(
        &self,
        original_id: &BillingInvoiceId,
    ) -> Result<Vec<InvoiceSummary>> {
        let rows = sqlx::query_as::<_, InvoiceRow>(&format!(
            "SELECT {INVOICE_COLS} FROM billing_invoices \
             WHERE tenant_id = $1 AND credits_invoice_id = $2 \
             ORDER BY created_at DESC, id"
        ))
        .bind(self.tenant.as_str())
        .bind(original_id.as_str())
        .fetch_all(&self.pool)
        .await
        .map_err(StoreError::Db)?;

        let figures = sqlx::query_as::<_, FiguresRow>(
            "SELECT l.invoice_id AS doc_id, l.qty_milli, l.unit_price_cents, l.vat_rate_bp \
             FROM billing_invoice_lines l \
             JOIN billing_invoices i ON i.tenant_id = l.tenant_id AND i.id = l.invoice_id \
             WHERE l.tenant_id = $1 AND i.credits_invoice_id = $2",
        )
        .bind(self.tenant.as_str())
        .bind(original_id.as_str())
        .fetch_all(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        let mut by_invoice = group_figures(figures);

        rows.into_iter()
            .map(|row| {
                let lines = by_invoice.remove(&row.id).unwrap_or_default();
                Ok(InvoiceSummary {
                    invoice: row.into_invoice()?,
                    totals: totals(&lines),
                    // A credit note cannot carry payments at all: money owed
                    // *to* the customer is not settled by them paying us, and
                    // [`crate::billing_payments`] refuses one. This is a fact
                    // about the document, not a column left unread.
                    paid_cents: 0,
                })
            })
            .collect()
    }

    /// Raises the **draft** invoice an accepted quote produces, inside the
    /// transaction that accepts it ([`AccountStore::accept_billing_quote`]).
    ///
    /// It writes only the header; the caller copies the quote's lines onto it
    /// under the same transaction, so acceptance either leaves a whole draft or
    /// leaves nothing at all. This lives here rather than in the quote module
    /// because `billing_invoices` is the one file that writes this table.
    ///
    /// What is copied from the offer and what is not:
    ///
    /// - **Customer and currency** are copied, not re-resolved. The offer was
    ///   made in them, so the invoice for it is raised in them — and copying is
    ///   also what lets an offer to a customer archived since it was sent still
    ///   be honoured, exactly as a credit note can still be raised for one.
    /// - **The customer's reference** (their RFQ number) is copied: it is the
    ///   customer's own thread of the transaction, and they will look for it on
    ///   the invoice.
    /// - **The note is not.** A quote's note states the terms of an *offer*
    ///   ("valid for fourteen days"), which is untrue the moment it becomes an
    ///   invoice. The draft is editable, so a caller that wants one writes it.
    /// - **Payment terms are the customer's today**, since a quote carries none
    ///   — the days an offer stands and the days a bill is owed in are
    ///   different facts — and they are then snapshotted on the document like
    ///   any other invoice's.
    ///
    /// # Errors
    /// [`StoreError::NotFound`] when the customer is gone (impossible while the
    /// quote's own foreign key holds); [`StoreError::Db`] on failure.
    pub(crate) async fn insert_invoice_from_quote(
        &self,
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
        source: &InvoiceFromQuote<'_>,
    ) -> Result<BillingInvoiceId> {
        let terms: Option<i32> = sqlx::query_scalar(
            "SELECT payment_terms_days FROM billing_customers WHERE tenant_id = $1 AND id = $2",
        )
        .bind(self.tenant.as_str())
        .bind(source.customer_id)
        .fetch_optional(&mut **tx)
        .await
        .map_err(StoreError::Db)?;
        let terms = terms.ok_or(StoreError::NotFound)?;

        let id = BillingInvoiceId::generate();
        sqlx::query(
            "INSERT INTO billing_invoices (tenant_id, id, customer_id, status, currency, \
                 payment_terms_days, quote_id, reference, note, created_by) \
             VALUES ($1, $2, $3, 'draft', $4, $5, $6, $7, '', $8)",
        )
        .bind(self.tenant.as_str())
        .bind(id.as_str())
        .bind(source.customer_id)
        .bind(source.currency)
        .bind(terms)
        .bind(source.quote_id)
        .bind(source.reference)
        .bind(self.user.as_str())
        .execute(&mut **tx)
        .await
        .map_err(StoreError::Db)?;
        Ok(id)
    }

    /// Raises the **draft** invoice one occurrence of a recurring arrangement
    /// produces (B2.11), inside the transaction that runs the schedule
    /// ([`AccountStore::run_billing_schedule`]).
    ///
    /// Like [`AccountStore::insert_invoice_from_quote`], it writes only the
    /// header and lives here because `billing_invoices` is the one file that
    /// writes this table; the caller copies the template's lines onto it under
    /// the same transaction, so a run either leaves a whole draft or leaves
    /// nothing at all.
    ///
    /// **Everything is copied from the arrangement, nothing re-resolved.** The
    /// customer, currency, terms, reference and note are the ones the schedule
    /// was set up with — a price list edited since must not change what a
    /// standing arrangement bills — and the customer is copied rather than
    /// re-checked, so an arrangement for a customer archived meanwhile still
    /// raises its draft and is visible to be dealt with, instead of failing
    /// silently in a background sweep nobody is watching.
    ///
    /// `created_by` is the schedule's owner, not whoever happened to trigger
    /// the run: it was their standing instruction that raised the document.
    ///
    /// The `due_date` the occurrence is *for* is stamped on the row and held
    /// unique per schedule by the database, so no period can be billed twice
    /// even if two runs raced past the row lock.
    ///
    /// # Errors
    /// [`StoreError::Db`] on failure, including the unique violation a second
    /// draft for the same occurrence would be.
    pub(crate) async fn insert_invoice_from_schedule(
        &self,
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
        source: &InvoiceFromSchedule<'_>,
    ) -> Result<BillingInvoiceId> {
        let id = BillingInvoiceId::generate();
        sqlx::query(
            "INSERT INTO billing_invoices (tenant_id, id, customer_id, status, currency, \
                 payment_terms_days, schedule_id, schedule_due_date, reference, note, created_by) \
             VALUES ($1, $2, $3, 'draft', $4, $5, $6, $7, $8, $9, $10)",
        )
        .bind(self.tenant.as_str())
        .bind(id.as_str())
        .bind(source.customer_id)
        .bind(source.currency)
        .bind(source.payment_terms_days)
        .bind(source.schedule_id)
        .bind(source.due_date)
        .bind(source.reference)
        .bind(source.note)
        .bind(source.created_by)
        .execute(&mut **tx)
        .await
        .map_err(StoreError::Db)?;
        Ok(id)
    }

    /// The drafts one of this tenant's arrangements has raised, newest
    /// occurrence first, each with its computed totals — the read behind "what
    /// has this schedule billed?".
    ///
    /// A schedule id that is absent or another tenant's yields an empty list,
    /// like every other list read here — never an existence oracle.
    ///
    /// # Errors
    /// [`StoreError::Db`] on failure.
    pub async fn billing_invoices_from_schedule(
        &self,
        schedule_id: &BillingScheduleId,
    ) -> Result<Vec<InvoiceSummary>> {
        let rows = sqlx::query_as::<_, InvoiceRow>(&format!(
            "SELECT {INVOICE_COLS} FROM billing_invoices \
             WHERE tenant_id = $1 AND schedule_id = $2 \
             ORDER BY schedule_due_date DESC, id"
        ))
        .bind(self.tenant.as_str())
        .bind(schedule_id.as_str())
        .fetch_all(&self.pool)
        .await
        .map_err(StoreError::Db)?;

        let figures = sqlx::query_as::<_, FiguresRow>(
            "SELECT l.invoice_id AS doc_id, l.qty_milli, l.unit_price_cents, l.vat_rate_bp \
             FROM billing_invoice_lines l \
             JOIN billing_invoices i ON i.tenant_id = l.tenant_id AND i.id = l.invoice_id \
             WHERE l.tenant_id = $1 AND i.schedule_id = $2",
        )
        .bind(self.tenant.as_str())
        .bind(schedule_id.as_str())
        .fetch_all(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        let mut by_invoice = group_figures(figures);

        let paid: Vec<(String, Option<i64>)> = sqlx::query_as(
            "SELECT p.invoice_id, sum(p.amount_cents)::bigint FROM billing_payments p \
             JOIN billing_invoices i ON i.tenant_id = p.tenant_id AND i.id = p.invoice_id \
             WHERE p.tenant_id = $1 AND i.schedule_id = $2 \
             GROUP BY p.invoice_id",
        )
        .bind(self.tenant.as_str())
        .bind(schedule_id.as_str())
        .fetch_all(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        let mut by_paid: HashMap<String, i64> = paid
            .into_iter()
            .map(|(id, sum)| (id, sum.unwrap_or(0)))
            .collect();

        rows.into_iter()
            .map(|row| {
                let lines = by_invoice.remove(&row.id).unwrap_or_default();
                let paid_cents = by_paid.remove(&row.id).unwrap_or(0);
                Ok(InvoiceSummary {
                    invoice: row.into_invoice()?,
                    totals: totals(&lines),
                    paid_cents,
                })
            })
            .collect()
    }

    /// The invoice raised from one of this tenant's quotes, or `None` — the
    /// read behind "was this offer billed?", and the link a quote's screen
    /// follows to the draft its acceptance produced.
    ///
    /// At most one exists: acceptance is terminal, and the database backs that
    /// with a unique index (migration 0106). A quote id that is absent or
    /// another tenant's yields `None`, like every other read here — never an
    /// existence oracle.
    ///
    /// # Errors
    /// [`StoreError::Db`] on failure.
    pub async fn billing_invoice_for_quote(
        &self,
        quote_id: &BillingQuoteId,
    ) -> Result<Option<BillingInvoiceId>> {
        let id: Option<String> = sqlx::query_scalar(
            "SELECT id FROM billing_invoices WHERE tenant_id = $1 AND quote_id = $2",
        )
        .bind(self.tenant.as_str())
        .bind(quote_id.as_str())
        .fetch_optional(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        Ok(id.map(BillingInvoiceId::new))
    }

    /// What a line set **would** total, without writing anything — the same
    /// arithmetic the stored document will report, so a draft editor can show
    /// live totals from the server rather than computing money in the browser.
    ///
    /// # Errors
    /// [`StoreError::Validation`] on the same rules as
    /// [`AccountStore::set_billing_invoice_lines`].
    pub fn billing_line_totals(&self, lines: &[NewLine]) -> Result<Totals> {
        let lines = normalize_lines(lines)?;
        let figures: Vec<LineFigures> = lines.iter().map(NormalizedLine::figures).collect();
        Ok(totals(&figures))
    }
}

// ---- row types --------------------------------------------------------------

/// The facts an accepted quote hands its invoice draft
/// ([`AccountStore::insert_invoice_from_quote`]).
///
/// Borrowed rather than owned: every field is read from the quote's own locked
/// row inside the accepting transaction and outlives the call.
#[derive(Debug)]
pub(crate) struct InvoiceFromQuote<'a> {
    /// The quote being accepted, which the new document points back to.
    pub(crate) quote_id: &'a str,
    /// The party quoted, copied so an archived customer can still be billed
    /// for an offer they accepted.
    pub(crate) customer_id: &'a str,
    /// The currency the offer was made in.
    pub(crate) currency: &'a str,
    /// The customer's own reference, as it appeared on the offer.
    pub(crate) reference: &'a str,
}

/// The facts one occurrence of a recurring arrangement hands its draft
/// ([`AccountStore::insert_invoice_from_schedule`]).
///
/// Borrowed rather than owned: every field is read from the schedule's own
/// locked row inside the running transaction and outlives the call.
#[derive(Debug)]
pub(crate) struct InvoiceFromSchedule<'a> {
    /// The arrangement that raised this draft, which it points back to.
    pub(crate) schedule_id: &'a str,
    /// The occurrence it is for — the date the arrangement was due on.
    pub(crate) due_date: Date,
    /// The party billed, copied so an archived customer's standing arrangement
    /// still raises a draft somebody can look at.
    pub(crate) customer_id: &'a str,
    /// The currency the arrangement was set up in.
    pub(crate) currency: &'a str,
    /// The terms it was set up with, snapshotted onto the document like any
    /// other invoice's.
    pub(crate) payment_terms_days: i32,
    /// The customer's own reference, as the arrangement carries it.
    pub(crate) reference: &'a str,
    /// The note printed under the lines, as the arrangement carries it.
    pub(crate) note: &'a str,
    /// The colleague whose standing instruction this is.
    pub(crate) created_by: &'a str,
}

/// What a locking read hands back: the stored facts a write decides against,
/// with the status already parsed.
#[derive(Debug)]
struct LockedInvoice {
    status: InvoiceStatus,
    is_credit_note: bool,
    credits_invoice_id: Option<String>,
    customer_id: String,
    currency: String,
    payment_terms_days: i32,
    reference: String,
}

#[derive(sqlx::FromRow)]
struct LockedRow {
    status: String,
    is_credit_note: bool,
    credits_invoice_id: Option<String>,
    customer_id: String,
    currency: String,
    payment_terms_days: i32,
    reference: String,
}

#[derive(sqlx::FromRow)]
struct InvoiceRow {
    id: String,
    customer_id: String,
    status: String,
    currency: String,
    number: Option<String>,
    issue_date: Option<Date>,
    due_date: Option<Date>,
    payment_terms_days: i32,
    is_credit_note: bool,
    credits_invoice_id: Option<String>,
    quote_id: Option<String>,
    schedule_id: Option<String>,
    schedule_due_date: Option<Date>,
    reference: String,
    note: String,
    fx_base_currency: Option<String>,
    fx_rate_micro: Option<i64>,
    fx_rate_date: Option<Date>,
    created_by: String,
    created_at: OffsetDateTime,
    updated_at: OffsetDateTime,
}

impl InvoiceRow {
    /// The rate snapshot, which exists only when all three of its columns do —
    /// a rate without the currency it converts into, or without the day it was
    /// published, is not something an auditor can recompute from, and the table
    /// constrains the three to move together.
    fn fx(&self) -> Option<FxSnapshot> {
        Some(FxSnapshot {
            base_currency: self.fx_base_currency.clone()?,
            rate_micro: self.fx_rate_micro?,
            rate_date: self.fx_rate_date?,
        })
    }

    fn into_invoice(self) -> Result<Invoice> {
        let status = parse_stored_status(&self.status)?;
        let fx = self.fx();
        Ok(Invoice {
            id: BillingInvoiceId::new(self.id),
            customer_id: BillingCustomerId::new(self.customer_id),
            status,
            currency: self.currency,
            number: self.number,
            issue_date: self.issue_date,
            due_date: self.due_date,
            payment_terms_days: self.payment_terms_days,
            is_credit_note: self.is_credit_note,
            credits_invoice_id: self.credits_invoice_id.map(BillingInvoiceId::new),
            quote_id: self.quote_id.map(BillingQuoteId::new),
            schedule_id: self.schedule_id.map(BillingScheduleId::new),
            schedule_due_date: self.schedule_due_date,
            reference: self.reference,
            note: self.note,
            fx,
            created_by: self.created_by,
            created_at: self.created_at,
            updated_at: self.updated_at,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_status_round_trips_through_its_stored_form() {
        for status in [
            InvoiceStatus::Draft,
            InvoiceStatus::Issued,
            InvoiceStatus::Paid,
            InvoiceStatus::Void,
        ] {
            assert_eq!(InvoiceStatus::parse(status.as_str()), Some(status));
        }
        assert!(InvoiceStatus::Draft.is_draft());
        for other in [
            InvoiceStatus::Issued,
            InvoiceStatus::Paid,
            InvoiceStatus::Void,
        ] {
            assert!(!other.is_draft());
        }
    }

    #[test]
    fn an_unknown_stored_status_is_never_guessed_at() {
        // Including near-misses: a document that says "Draft" or "sent" is
        // corrupt data, and treating it as a draft would make a frozen
        // document editable.
        for bad in ["", "Draft", "DRAFT", "sent", "cancelled", "issued "] {
            assert_eq!(InvoiceStatus::parse(bad), None, "{bad:?}");
        }
    }

    #[test]
    fn only_a_draft_may_be_changed() {
        assert!(
            InvoiceStatus::Draft.ensure_editable().is_ok(),
            "a draft is what the editor edits"
        );
        for frozen in [
            InvoiceStatus::Issued,
            InvoiceStatus::Paid,
            InvoiceStatus::Void,
        ] {
            match frozen.ensure_editable() {
                Err(StoreError::Conflict(message)) => {
                    // The refusal says which state refused, so the UI can tell
                    // "already issued" from "already void" without a second
                    // round trip — and carries no other document's data.
                    assert!(
                        message.contains(frozen.as_str()),
                        "{message:?} should name {frozen:?}"
                    );
                    assert!(message.contains("draft"), "{message:?}");
                }
                other => panic!("expected Conflict for {frozen:?}, got {other:?}"),
            }
        }
    }

    #[test]
    fn a_corrupt_stored_status_is_a_decode_failure_not_a_guess() {
        // Never `Validation` (that would blame the caller) and never a
        // silently-editable draft: the row is unreadable, and the reason stays
        // in the error's source rather than its message.
        match parse_stored_status("sent") {
            Err(StoreError::Db(_)) => {}
            other => panic!("expected a decode failure, got {other:?}"),
        }
        assert!(parse_stored_status("draft").is_ok());
    }

    #[test]
    fn only_a_document_the_customer_holds_may_be_credited() {
        for creditable in [InvoiceStatus::Issued, InvoiceStatus::Paid] {
            assert!(
                creditable.ensure_creditable().is_ok(),
                "{creditable:?} is a document the customer holds"
            );
        }
        // A draft was never a document, and a void one is already cancelled in
        // full — crediting either would take the ledger below zero. Both
        // refusals say which case they are, so the UI can offer "delete" or
        // say "already cancelled" without a second round trip.
        let draft = match InvoiceStatus::Draft.ensure_creditable() {
            Err(StoreError::Conflict(message)) => message,
            other => panic!("expected Conflict for a draft, got {other:?}"),
        };
        assert!(
            draft.contains("draft") && draft.contains("delete"),
            "{draft}"
        );
        let void = match InvoiceStatus::Void.ensure_creditable() {
            Err(StoreError::Conflict(message)) => message,
            other => panic!("expected Conflict for a void document, got {other:?}"),
        };
        assert!(
            void.contains("void") && void.contains("cancelled"),
            "{void}"
        );
    }

    /// A header in a given state, for the overdue predicate: everything else
    /// about a document is irrelevant to it.
    fn dated(status: InvoiceStatus, due: Option<Date>) -> Invoice {
        Invoice {
            id: BillingInvoiceId::new("inv"),
            customer_id: BillingCustomerId::new("cust"),
            status,
            currency: "EUR".to_owned(),
            number: Some("INV-2026-00001".to_owned()),
            issue_date: due,
            due_date: due,
            payment_terms_days: 14,
            is_credit_note: false,
            credits_invoice_id: None,
            quote_id: None,
            schedule_id: None,
            schedule_due_date: None,
            reference: String::new(),
            note: String::new(),
            fx: due.map(|day| FxSnapshot::identity("EUR", day)),
            created_by: "u".to_owned(),
            created_at: OffsetDateTime::UNIX_EPOCH,
            updated_at: OffsetDateTime::UNIX_EPOCH,
        }
    }

    #[test]
    fn only_an_issued_document_past_its_date_is_overdue() {
        let today = Date::from_calendar_date(2026, time::Month::August, 6)
            .unwrap_or_else(|e| panic!("{e}"));
        let day_before = today
            .previous_day()
            .unwrap_or_else(|| panic!("no yesterday"));
        let day_after = today.next_day().unwrap_or_else(|| panic!("no tomorrow"));

        assert!(dated(InvoiceStatus::Issued, Some(day_before)).is_overdue(today));
        // Due *today* is not yet late: the customer has the whole day.
        assert!(!dated(InvoiceStatus::Issued, Some(today)).is_overdue(today));
        assert!(!dated(InvoiceStatus::Issued, Some(day_after)).is_overdue(today));
        // No due date at all (a draft's shape) is never overdue, whatever the
        // status column says.
        assert!(!dated(InvoiceStatus::Issued, None).is_overdue(today));
        // Settled, cancelled, or never a document: none of them is owed.
        for other in [
            InvoiceStatus::Draft,
            InvoiceStatus::Paid,
            InvoiceStatus::Void,
        ] {
            assert!(
                !dated(other, Some(day_before)).is_overdue(today),
                "{other:?} is not owed and cannot be overdue"
            );
        }

        // A credit note is money owed *to* the customer: a date passing on it
        // makes nobody late, so it is never overdue in any state.
        let mut credit = dated(InvoiceStatus::Issued, Some(day_before));
        credit.is_credit_note = true;
        credit.credits_invoice_id = Some(BillingInvoiceId::new("original"));
        assert!(!credit.is_overdue(today));
    }

    #[test]
    fn a_documents_settlement_reads_from_its_totals_and_its_payments() {
        // The one place the two facts meet: nothing here is stored, so a
        // summary and the document it summarises cannot disagree.
        let summary = InvoiceSummary {
            invoice: dated(InvoiceStatus::Issued, None),
            totals: Totals {
                net_cents: 100_000,
                vat_cents: 21_000,
                gross_cents: 121_000,
                vat_by_rate: Vec::new(),
            },
            paid_cents: 21_000,
        };
        let settlement = summary.settlement();
        assert_eq!(settlement.gross_cents, 121_000);
        assert_eq!(settlement.paid_cents, 21_000);
        assert_eq!(settlement.outstanding_cents, 100_000);
        assert_eq!(
            settlement.state,
            crate::billing_payments::PaymentState::PartiallyPaid
        );
    }

    #[test]
    fn a_new_invoice_defaults_to_the_customers_own_terms() {
        let input = NewInvoice::for_customer(BillingCustomerId::new("cust"));
        assert!(
            input.currency.is_none() && input.payment_terms_days.is_none(),
            "None means: take the customer's"
        );
        assert!(input.reference.is_empty() && input.note.is_empty());
    }
}
