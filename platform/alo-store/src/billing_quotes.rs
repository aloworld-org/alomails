//! Billing quotes — the offer that precedes an invoice (alo Billing, ADR 0035,
//! wave B1), reached through the account door like [`crate::billing_invoices`].
//!
//! A quote has the same two halves of a life as an invoice: a **draft** that is
//! freely editable and carries no number, and a document that has left the
//! building and is frozen. **Sending** is the transition that draws the next
//! number from this tenant's quote series ([`crate::billing_sequence`]), stamps
//! the send date, derives the validity date from the days snapshotted on the
//! document, and freezes the content.
//!
//! **The lifecycle is `draft → sent → accepted | declined | expired`**, and
//! nothing else. Every transition goes through one pure table
//! ([`QuoteStatus::can_advance_to`]), so the rules are unit-tested over all
//! twenty-five ordered pairs rather than being scattered across the write
//! paths. The three closing states are **terminal**: an offer that was declined
//! or that lapsed is not re-opened by editing a status — the answer to "they
//! want it after all" is a new quote, which is also what keeps a document a
//! customer holds and the record of what they were offered the same thing.
//!
//! **Expiry is both a fact and a decision.** [`Quote::is_expired`] derives, on
//! every read, whether a sent offer is past its date — like an invoice's
//! overdue flag, and for the same reason: a stored flag would be wrong every
//! midnight. Moving the quote to `expired` is a separate, recorded act, so a
//! tenant that chooses to honour a lapsed offer simply accepts it. There is
//! deliberately no background sweep that closes quotes on their own.
//!
//! **The line model is shared with invoices** ([`crate::billing_line`]) —
//! literally the same types, the same field rules and the same statements over
//! a second table — because a quote's line and an invoice's line are the same
//! thing. That is what makes copying an accepted quote onto an invoice draft
//! (B1.12) a copy rather than a translation. The two documents keep separate
//! tables and separate modules because their *lives* differ: an invoice is owed
//! money under a legally gapless number, a quote is an offer that can simply be
//! turned down.
//!
//! Money is never stored: net, VAT and gross are derived from the lines on
//! every read by [`crate::billing_totals`], so a total can never drift from the
//! lines that justify it.
//!
//! Tenancy is structural: every statement carries `tenant_id` from the handle,
//! the customer link is re-checked under the same handle before it is written,
//! and the database backs that with a composite foreign key on
//! `(tenant_id, customer_id)`.

use time::{Date, Duration, OffsetDateTime};

use crate::account::AccountStore;
use crate::billing_field::{bounded, currency};
use crate::billing_invoices::{
    INVOICE_NOTE_MAX_CHARS, INVOICE_REFERENCE_MAX_CHARS, InvoiceFromQuote,
};
use crate::billing_line::{FiguresRow, INVOICE_LINES, group_figures};
use crate::billing_quote_lines::{self, NewQuoteLine, QuoteLine};
use crate::billing_sequence::{
    QUOTE_NUMBER_PREFIX, QUOTE_SEQUENCE_KIND, document_number, draw_next,
};
use crate::billing_totals::{LineFigures, Totals, totals};
use crate::error::{Result, StoreError};
use crate::id::{BillingCustomerId, BillingInvoiceId, BillingProductId, BillingQuoteId};

/// How long an offer stands when the caller states nothing — a month, the
/// common European B2B habit.
pub const DEFAULT_QUOTE_VALID_DAYS: i32 = 30;

/// The longest validity we accept, in days. A year is already far beyond any
/// real offer; anything longer is a typo.
pub const QUOTE_VALID_MAX_DAYS: i32 = 365;

/// The columns every read of a quote selects, in `QuoteRow` order.
const QUOTE_COLS: &str = "id, customer_id, status, currency, number, sent_date, valid_until, \
     valid_days, decided_date, reference, note, created_by, created_at, updated_at";

/// Where an offer is in its life.
///
/// `draft → sent → accepted | declined | expired`; the three closing states are
/// terminal. A draft is deleted rather than closed, because it was never an
/// offer to anybody.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum QuoteStatus {
    /// Editable, unnumbered, not yet offered to anyone.
    Draft,
    /// Numbered, dated and frozen; the offer is open.
    Sent,
    /// The customer took the offer. B1.12 turns it into a draft invoice.
    Accepted,
    /// The customer turned the offer down.
    Declined,
    /// The offer lapsed without an answer.
    Expired,
}

impl QuoteStatus {
    /// The value stored in the `status` column.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Draft => "draft",
            Self::Sent => "sent",
            Self::Accepted => "accepted",
            Self::Declined => "declined",
            Self::Expired => "expired",
        }
    }

    /// Parses a stored status, or `None` if it is not one we know.
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "draft" => Some(Self::Draft),
            "sent" => Some(Self::Sent),
            "accepted" => Some(Self::Accepted),
            "declined" => Some(Self::Declined),
            "expired" => Some(Self::Expired),
            _ => None,
        }
    }

    /// Whether the offer is still editable.
    pub fn is_draft(self) -> bool {
        matches!(self, Self::Draft)
    }

    /// Whether the offer is closed — decided one way or the other, and
    /// therefore stamped with a decision date.
    pub fn is_closed(self) -> bool {
        matches!(self, Self::Accepted | Self::Declined | Self::Expired)
    }

    /// The states this one may move to. **The whole lifecycle is this
    /// function**: a draft is offered, an open offer is answered, and an
    /// answered one is finished with.
    pub fn allowed_next(self) -> &'static [Self] {
        match self {
            Self::Draft => &[Self::Sent],
            Self::Sent => &[Self::Accepted, Self::Declined, Self::Expired],
            Self::Accepted | Self::Declined | Self::Expired => &[],
        }
    }

    /// Whether `to` is a legal move from this state. Never true for `to ==
    /// self`: re-sending an already-sent quote, or accepting one twice, is a
    /// caller that has lost track of the document, and answering "fine" would
    /// hide that (and, for sending, would draw a second number).
    pub fn can_advance_to(self, to: Self) -> bool {
        self.allowed_next().contains(&to)
    }

    /// The guard every transition runs.
    ///
    /// The refusal names both states **and** what this state does allow, so a
    /// UI can correct itself without a second round trip.
    ///
    /// # Errors
    /// [`StoreError::Conflict`] (`409` at the route edge) when the move is not
    /// in [`QuoteStatus::allowed_next`].
    pub fn ensure_transition(self, to: Self) -> Result<()> {
        if self.can_advance_to(to) {
            return Ok(());
        }
        let allowed: Vec<&str> = self.allowed_next().iter().map(|s| s.as_str()).collect();
        let allowance = if allowed.is_empty() {
            "it is closed and cannot change again".to_owned()
        } else {
            format!(
                "from {} it can only become {}",
                self.as_str(),
                allowed.join(" or ")
            )
        };
        Err(StoreError::Conflict(format!(
            "a quote cannot become {} while it is {}; {allowance}",
            to.as_str(),
            self.as_str()
        )))
    }

    /// The guard every write path runs before it changes a quote's content: a
    /// draft may be edited and deleted, anything else is frozen.
    ///
    /// A sent quote is a document the customer holds; editing it would change
    /// what they were offered without either party's copy saying so. The offer
    /// is withdrawn (declined) and a new one made.
    ///
    /// # Errors
    /// [`StoreError::Conflict`] naming the status that refused (`409`).
    pub fn ensure_editable(self) -> Result<()> {
        if self.is_draft() {
            return Ok(());
        }
        Err(StoreError::Conflict(format!(
            "a quote can only be changed while it is a draft; this one is {}",
            self.as_str()
        )))
    }
}

/// Turns a stored status string into a status, or reports corrupt data.
///
/// A status the code does not know is corrupt data, not user input: it is
/// reported as a decode failure (detail in the source, never in the message)
/// rather than guessed at, because guessing here would mean treating a frozen
/// document as editable.
fn parse_stored_status(stored: &str) -> Result<QuoteStatus> {
    QuoteStatus::parse(stored).ok_or_else(|| {
        StoreError::Db(sqlx::Error::Decode(
            "billing_quotes.status is not a known status".into(),
        ))
    })
}

/// Validates how long an offer stands, in days. Zero is valid — an offer good
/// only on the day it is made.
fn valid_days(value: i32) -> Result<i32> {
    if !(0..=QUOTE_VALID_MAX_DAYS).contains(&value) {
        return Err(StoreError::Validation(format!(
            "quote validity must be between 0 and {QUOTE_VALID_MAX_DAYS} days"
        )));
    }
    Ok(value)
}

/// The writable header of a quote, used for both create and update (an update
/// is a full replace — the route layer merges a partial `PATCH` onto the stored
/// record before calling). Lines are written separately, as a set.
///
/// `currency` is `None` to mean *take the customer's*, and `valid_days` `None`
/// to mean [`DEFAULT_QUOTE_VALID_DAYS`]. Whatever is resolved is **stored on
/// the document**: changing a habit next year must not restate an offer made
/// this year.
#[derive(Debug, Clone)]
pub struct NewQuote {
    /// The party quoted. Must be one of this tenant's customers.
    pub customer_id: BillingCustomerId,
    /// ISO 4217 code, or `None` for the customer's default.
    pub currency: Option<String>,
    /// Days the offer stands from the send date, or `None` for the default.
    pub valid_days: Option<i32>,
    /// The customer's own reference (an RFQ number), printed on the document.
    pub reference: String,
    /// Free-text note printed under the lines.
    pub note: String,
}

impl NewQuote {
    /// The blank header a new draft starts from: this customer, their currency,
    /// the default validity, no reference and no note. There is deliberately no
    /// [`Default`] — a quote without a customer is not a document.
    pub fn for_customer(customer_id: BillingCustomerId) -> Self {
        Self {
            customer_id,
            currency: None,
            valid_days: None,
            reference: String::new(),
            note: String::new(),
        }
    }
}

/// The header of a stored quote. Its money lives in [`Totals`], computed from
/// the lines.
#[derive(Debug, Clone)]
pub struct Quote {
    /// Opaque id, unique within the tenant.
    pub id: BillingQuoteId,
    /// The party quoted.
    pub customer_id: BillingCustomerId,
    /// Where the offer is in its life.
    pub status: QuoteStatus,
    /// ISO 4217 code the offer was made in.
    pub currency: String,
    /// The document number, `None` while draft.
    pub number: Option<String>,
    /// The day the offer was made, `None` while draft.
    pub sent_date: Option<Date>,
    /// The last day the offer stands, `None` while draft.
    pub valid_until: Option<Date>,
    /// How long the offer stands, in days from the send date.
    pub valid_days: i32,
    /// The day the offer was accepted, declined or expired; `None` while it is
    /// still open (or was never made).
    pub decided_date: Option<Date>,
    /// The customer's own reference.
    pub reference: String,
    /// Free-text note.
    pub note: String,
    /// The user who created the document.
    pub created_by: String,
    /// Creation time.
    pub created_at: OffsetDateTime,
    /// Last modification time — moved by a header edit, a line edit and every
    /// transition, since all three change what the document says.
    pub updated_at: OffsetDateTime,
}

impl Quote {
    /// Whether the offer has **lapsed** as of `today`: sent, still open, and
    /// past the validity date it was stamped with.
    ///
    /// Derived, never stored — a stored flag would be wrong every midnight, and
    /// the two facts it is derived from are frozen on the document already. A
    /// draft was never offered, and a closed quote already has its answer;
    /// neither is ever "lapsed", whatever the dates say. Moving such a quote to
    /// [`QuoteStatus::Expired`] is a separate, recorded decision.
    pub fn is_expired(&self, today: Date) -> bool {
        matches!(self.status, QuoteStatus::Sent)
            && self.valid_until.is_some_and(|until| until < today)
    }
}

/// A quote as a list entry: the header and what it is worth, without the lines.
/// The totals are computed, never read from a column.
#[derive(Debug, Clone)]
pub struct QuoteSummary {
    /// The header.
    pub quote: Quote,
    /// Net, VAT breakdown and gross, derived from the lines.
    pub totals: Totals,
}

/// A whole quote: header, lines in print order, and the totals derived from
/// those lines.
#[derive(Debug, Clone)]
pub struct QuoteDocument {
    /// The header.
    pub quote: Quote,
    /// The lines, in print order.
    pub lines: Vec<QuoteLine>,
    /// Net, VAT breakdown and gross, derived from `lines`.
    pub totals: Totals,
}

/// What accepting an offer produces: the closed quote, and the document raised
/// from it in the same transaction.
///
/// Two values rather than one because they are two documents, and a caller that
/// only wanted to record the answer still gets what its acceptance created —
/// there is no second call that could tell it.
#[derive(Debug, Clone)]
pub struct QuoteAcceptance {
    /// The quote, now `accepted` and stamped with the day it was decided.
    pub quote: QuoteDocument,
    /// What the offer became.
    pub outcome: AcceptedAs,
}

/// The document an accepted offer became — **an enum rather than two nullable
/// ids**, so a caller cannot read the one that was not raised.
///
/// Which it is depends on the lines (ADR 0054 §5): an offer naming any catalog
/// item is goods and becomes an order, because goods are reserved, picked and
/// delivered before anybody is billed; an offer of services becomes the draft
/// invoice it always did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AcceptedAs {
    /// A draft invoice carrying a copy of the offer's lines — the services path,
    /// unchanged since B1.12.
    InvoiceDraft(BillingInvoiceId),
    /// A **draft** sales order carrying the offer's lines and their products.
    /// Confirming it is a separate act, so accepting an offer never quietly
    /// commits stock.
    SalesOrder(crate::id::InvSalesOrderId),
}

impl AcceptedAs {
    /// The draft invoice, when that is what was raised.
    pub fn invoice_id(&self) -> Option<&BillingInvoiceId> {
        match self {
            Self::InvoiceDraft(id) => Some(id),
            Self::SalesOrder(_) => None,
        }
    }

    /// The draft sales order, when that is what was raised.
    pub fn sales_order_id(&self) -> Option<&crate::id::InvSalesOrderId> {
        match self {
            Self::SalesOrder(id) => Some(id),
            Self::InvoiceDraft(_) => None,
        }
    }
}

/// The header, validated and with the customer's defaults resolved.
#[derive(Debug)]
struct NormalizedQuote {
    customer_id: String,
    currency: String,
    valid_days: i32,
    reference: String,
    note: String,
}

/// What a locking read hands back: the stored facts a write decides against,
/// with the status already parsed.
///
/// It reads more than the status because acceptance copies the offer onto an
/// invoice draft under the same lock, and what it copies must be the row as it
/// stood when the transition was allowed — not a second read that a concurrent
/// writer could have moved.
#[derive(Debug)]
struct LockedQuote {
    status: QuoteStatus,
    valid_days: i32,
    customer_id: String,
    currency: String,
    reference: String,
}

impl AccountStore {
    /// Resolves a header against **this tenant's** customer: the customer must
    /// exist under this handle, so a guessed id from another tenant is a
    /// `NotFound`, and it must be active — archiving a customer means "we no
    /// longer do business with them", so offering them something new is a
    /// mistake worth reporting rather than obeying.
    async fn normalize_quote(&self, input: &NewQuote) -> Result<NormalizedQuote> {
        let customer = self
            .billing_customer(&input.customer_id)
            .await?
            .ok_or(StoreError::NotFound)?;
        if customer.is_archived() {
            return Err(StoreError::Validation(
                "the customer is archived; restore it before quoting them again".to_owned(),
            ));
        }
        let resolved_currency = match input.currency.as_deref() {
            Some(code) => currency(code)?,
            None => customer.currency,
        };
        Ok(NormalizedQuote {
            customer_id: customer.id.as_str().to_owned(),
            currency: resolved_currency,
            valid_days: valid_days(input.valid_days.unwrap_or(DEFAULT_QUOTE_VALID_DAYS))?,
            reference: bounded("reference", &input.reference, INVOICE_REFERENCE_MAX_CHARS)?,
            note: bounded("note", &input.note, INVOICE_NOTE_MAX_CHARS)?,
        })
    }

    /// Takes the quote's row lock inside `tx` and returns the stored facts a
    /// write has to decide against, so a caller can check whether it may write
    /// and then write it without any other transaction slipping in between. Two
    /// writers to one quote serialise here; a writer that arrives after a send
    /// sees `sent`.
    ///
    /// # Errors
    /// [`StoreError::NotFound`] when the id is absent **or another tenant's**;
    /// [`StoreError::Db`] on failure or on a status the code does not know.
    async fn lock_quote(
        &self,
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
        id: &BillingQuoteId,
    ) -> Result<LockedQuote> {
        let row: Option<LockedRow> = sqlx::query_as(
            "SELECT status, valid_days, customer_id, currency, reference FROM billing_quotes \
             WHERE tenant_id = $1 AND id = $2 FOR UPDATE",
        )
        .bind(self.tenant.as_str())
        .bind(id.as_str())
        .fetch_optional(&mut **tx)
        .await
        .map_err(StoreError::Db)?;
        let row = row.ok_or(StoreError::NotFound)?;
        Ok(LockedQuote {
            status: parse_stored_status(&row.status)?,
            valid_days: row.valid_days,
            customer_id: row.customer_id,
            currency: row.currency,
            reference: row.reference,
        })
    }

    /// The status of one of this tenant's quotes, without taking a lock — the
    /// cheap pre-check that lets a write refuse a frozen document before it
    /// does any other work. It is never the authority: every write re-reads the
    /// status under [`AccountStore::lock_quote`] before writing.
    ///
    /// # Errors
    /// [`StoreError::NotFound`] when the id is absent or another tenant's;
    /// [`StoreError::Db`] on failure.
    pub(crate) async fn quote_status(&self, id: &BillingQuoteId) -> Result<QuoteStatus> {
        let stored: Option<String> = sqlx::query_scalar(
            "SELECT status FROM billing_quotes WHERE tenant_id = $1 AND id = $2",
        )
        .bind(self.tenant.as_str())
        .bind(id.as_str())
        .fetch_optional(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        parse_stored_status(&stored.ok_or(StoreError::NotFound)?)
    }

    /// Creates a **draft** quote with no lines — the state a new offer starts
    /// in. It carries no number and no dates by construction; only sending
    /// assigns those.
    ///
    /// # Errors
    /// [`StoreError::NotFound`] when the customer is not this tenant's;
    /// [`StoreError::Validation`] when the customer is archived or a header
    /// field breaks its rule; [`StoreError::Db`] on failure.
    pub async fn create_billing_quote(&self, input: &NewQuote) -> Result<BillingQuoteId> {
        let header = self.normalize_quote(input).await?;
        let id = BillingQuoteId::generate();
        sqlx::query(
            "INSERT INTO billing_quotes (tenant_id, id, customer_id, status, currency, \
                 valid_days, reference, note, created_by) \
             VALUES ($1, $2, $3, 'draft', $4, $5, $6, $7, $8)",
        )
        .bind(self.tenant.as_str())
        .bind(id.as_str())
        .bind(&header.customer_id)
        .bind(&header.currency)
        .bind(header.valid_days)
        .bind(&header.reference)
        .bind(&header.note)
        .bind(self.user.as_str())
        .execute(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        Ok(id)
    }

    /// The tenant's quotes, newest first, each with its computed totals.
    /// `status` filters; `None` lists everything.
    ///
    /// The lines of every listed quote are fetched in one further statement,
    /// not one per quote.
    ///
    /// # Errors
    /// [`StoreError::Db`] on failure.
    pub async fn billing_quotes(&self, status: Option<QuoteStatus>) -> Result<Vec<QuoteSummary>> {
        let status = status.map(QuoteStatus::as_str);
        let rows = sqlx::query_as::<_, QuoteRow>(&format!(
            "SELECT {QUOTE_COLS} FROM billing_quotes \
             WHERE tenant_id = $1 AND ($2::text IS NULL OR status = $2) \
             ORDER BY created_at DESC, id"
        ))
        .bind(self.tenant.as_str())
        .bind(status)
        .fetch_all(&self.pool)
        .await
        .map_err(StoreError::Db)?;

        let figures = sqlx::query_as::<_, FiguresRow>(
            "SELECT quote_id AS doc_id, qty_milli, unit_price_cents, vat_rate_bp \
             FROM billing_quote_lines \
             WHERE tenant_id = $1 AND quote_id IN ( \
                 SELECT id FROM billing_quotes \
                 WHERE tenant_id = $1 AND ($2::text IS NULL OR status = $2))",
        )
        .bind(self.tenant.as_str())
        .bind(status)
        .fetch_all(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        let mut by_quote = group_figures(figures);

        rows.into_iter()
            .map(|row| {
                let lines = by_quote.remove(&row.id).unwrap_or_default();
                Ok(QuoteSummary {
                    quote: row.into_quote()?,
                    totals: totals(&lines),
                })
            })
            .collect()
    }

    /// One quote of the tenant with its lines and totals, or `None` — including
    /// when the id belongs to another tenant (indistinguishable by design).
    ///
    /// # Errors
    /// [`StoreError::Db`] on failure.
    pub async fn billing_quote(&self, id: &BillingQuoteId) -> Result<Option<QuoteDocument>> {
        let Some(row) = sqlx::query_as::<_, QuoteRow>(&format!(
            "SELECT {QUOTE_COLS} FROM billing_quotes WHERE tenant_id = $1 AND id = $2"
        ))
        .bind(self.tenant.as_str())
        .bind(id.as_str())
        .fetch_optional(&self.pool)
        .await
        .map_err(StoreError::Db)?
        else {
            return Ok(None);
        };
        let lines =
            billing_quote_lines::read(&self.pool, self.tenant.as_str(), id.as_str()).await?;
        let figures: Vec<LineFigures> = lines.iter().map(|l| l.line.figures()).collect();
        Ok(Some(QuoteDocument {
            quote: row.into_quote()?,
            lines,
            totals: totals(&figures),
        }))
    }

    /// The id of the tenant's quote **numbered** `number`, or `None`.
    ///
    /// The counterpart of [`AccountStore::billing_invoice_id_by_number`], and
    /// for the same reason: the billing agent (B1.25) is given the number a
    /// person says ("QUO-2026-00001"), never an opaque id. Case-insensitive,
    /// blanks trimmed, otherwise exact; a draft has no number and is therefore
    /// unreachable here; another tenant's number is `None`.
    ///
    /// # Errors
    /// [`StoreError::Db`] on failure.
    pub async fn billing_quote_id_by_number(&self, number: &str) -> Result<Option<BillingQuoteId>> {
        let wanted = number.trim();
        if wanted.is_empty() {
            return Ok(None);
        }
        let id: Option<String> = sqlx::query_scalar(
            "SELECT id FROM billing_quotes WHERE tenant_id = $1 AND upper(number) = upper($2)",
        )
        .bind(self.tenant.as_str())
        .bind(wanted)
        .fetch_optional(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        Ok(id.map(BillingQuoteId::new))
    }

    /// Replaces the writable header of a **draft** quote: customer, currency,
    /// validity, reference and note. Status, number and dates are not writable
    /// here — they move only through the lifecycle actions.
    ///
    /// The status is checked before the header is even validated, so a frozen
    /// document is told it is frozen rather than being handed a complaint about
    /// a field it was never going to accept; it is then re-checked under the row
    /// lock that the write itself takes.
    ///
    /// # Errors
    /// [`StoreError::NotFound`] when the quote or the customer is not this
    /// tenant's; [`StoreError::Conflict`] when the quote is no longer a draft;
    /// [`StoreError::Validation`] as for create; [`StoreError::Db`] on failure.
    pub async fn update_billing_quote(&self, id: &BillingQuoteId, input: &NewQuote) -> Result<()> {
        self.quote_status(id).await?.ensure_editable()?;
        let header = self.normalize_quote(input).await?;

        let mut tx = self.pool.begin().await.map_err(StoreError::Db)?;
        // Authoritative: the state that matters is the one under the lock the
        // UPDATE below writes through, not the one read a moment ago.
        // Dropping the transaction on any error rolls it back untouched.
        self.lock_quote(&mut tx, id)
            .await?
            .status
            .ensure_editable()?;
        sqlx::query(
            "UPDATE billing_quotes SET customer_id = $3, currency = $4, valid_days = $5, \
                 reference = $6, note = $7, updated_at = now() \
             WHERE tenant_id = $1 AND id = $2",
        )
        .bind(self.tenant.as_str())
        .bind(id.as_str())
        .bind(&header.customer_id)
        .bind(&header.currency)
        .bind(header.valid_days)
        .bind(&header.reference)
        .bind(&header.note)
        .execute(&mut *tx)
        .await
        .map_err(StoreError::Db)?;
        tx.commit().await.map_err(StoreError::Db)?;
        Ok(())
    }

    /// Replaces the whole line set of a **draft** quote, in the caller's order,
    /// in one transaction: either the document reads exactly as the caller sent
    /// it or it is untouched. Line positions are assigned 0-based from that
    /// order, so what was sent is what prints.
    ///
    /// Every line is validated before anything is written, and the draft-only
    /// guard runs before even that, under the same lock the replacement writes
    /// through — so a set cannot land on a quote that was sent while it was
    /// being composed.
    ///
    /// # Errors
    /// [`StoreError::NotFound`] when the quote is not this tenant's;
    /// [`StoreError::Conflict`] when it is no longer a draft;
    /// [`StoreError::Validation`] when the set is too long or a line breaks a
    /// field rule (the message names the line's position);
    /// [`StoreError::Db`] on failure.
    pub async fn set_billing_quote_lines(
        &self,
        id: &BillingQuoteId,
        lines: &[NewQuoteLine],
    ) -> Result<()> {
        // Every product the offer names is held to **this tenant's** catalog
        // before a row is written — the same discipline `inv_so` applies to an
        // order's lines, and the reason a guessed id from elsewhere is a clean
        // `NotFound` rather than a foreign-key error.
        // **The state is checked first, and the order matters.** A frozen quote
        // refuses an edit whatever the payload says — the state is the reason,
        // and it outranks any complaint about content, which is what a caller
        // needs to hear to stop trying. Validating first would answer a sent
        // quote with "line 1: description is required", sending somebody to fix
        // a document that cannot be edited at all.
        let mut tx = self.pool.begin().await.map_err(StoreError::Db)?;
        self.lock_quote(&mut tx, id)
            .await?
            .status
            .ensure_editable()?;
        let normalized = billing_quote_lines::normalize_quote_lines(lines)?;
        self.check_quote_line_products(&normalized).await?;
        billing_quote_lines::replace(&mut tx, self.tenant.as_str(), id.as_str(), &normalized)
            .await?;
        sqlx::query(
            "UPDATE billing_quotes SET updated_at = now() WHERE tenant_id = $1 AND id = $2",
        )
        .bind(self.tenant.as_str())
        .bind(id.as_str())
        .execute(&mut *tx)
        .await
        .map_err(StoreError::Db)?;
        tx.commit().await.map_err(StoreError::Db)?;
        Ok(())
    }

    /// Holds every product a line set names to **this tenant's** catalog.
    ///
    /// A product that is not ours is a [`StoreError::NotFound`] — existence is
    /// never disclosed across tenants — and an archived one is a
    /// [`StoreError::Validation`] naming the line, because archiving means the
    /// tenant has stopped carrying it and offering more is a mistake worth
    /// reporting. Both are decided before a single row is written.
    ///
    /// This is `inv_so`'s own `normalize_so_lines` check, applied to the offer
    /// for the same reason: the product on a quote line becomes the product on
    /// an order line, and an order line pointing at somebody else's item is not
    /// a document anybody can deliver.
    async fn check_quote_line_products(
        &self,
        lines: &[crate::billing_quote_lines::NormalizedQuoteLine],
    ) -> Result<()> {
        let named = billing_quote_lines::products_named(lines);
        if named.is_empty() {
            return Ok(());
        }
        let rows: Vec<(String, bool)> = sqlx::query_as(
            "SELECT id, archived_at IS NOT NULL FROM billing_products \
             WHERE tenant_id = $1 AND id = ANY($2)",
        )
        .bind(self.tenant.as_str())
        .bind(&named)
        .fetch_all(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        for (position, line) in lines.iter().enumerate() {
            let Some(product) = line.product_id.as_deref() else {
                continue;
            };
            let found = rows.iter().find(|(id, _)| id == product);
            match found {
                None => return Err(StoreError::NotFound),
                Some((_, true)) => {
                    return Err(StoreError::Validation(format!(
                        "line {}: that item is archived; restore it before offering it again",
                        position + 1
                    )));
                }
                Some((_, false)) => {}
            }
        }
        Ok(())
    }

    /// Deletes a **draft** quote and, by cascade, its lines. This is the only
    /// quote that is ever removed: a draft never consumed a number and was
    /// never offered to anybody. A sent one is closed (declined or expired)
    /// instead, keeping its number and its content readable.
    ///
    /// # Errors
    /// [`StoreError::NotFound`] when the quote is absent or another tenant's;
    /// [`StoreError::Conflict`] when it is no longer a draft;
    /// [`StoreError::Db`] on failure.
    pub async fn delete_billing_quote(&self, id: &BillingQuoteId) -> Result<()> {
        let mut tx = self.pool.begin().await.map_err(StoreError::Db)?;
        self.lock_quote(&mut tx, id)
            .await?
            .status
            .ensure_editable()?;
        sqlx::query("DELETE FROM billing_quotes WHERE tenant_id = $1 AND id = $2")
            .bind(self.tenant.as_str())
            .bind(id.as_str())
            .execute(&mut *tx)
            .await
            .map_err(StoreError::Db)?;
        tx.commit().await.map_err(StoreError::Db)?;
        Ok(())
    }

    /// Sends a **draft** quote: draws the next number from this tenant's quote
    /// series, stamps the send date and the date the offer stands until, and
    /// freezes the content.
    ///
    /// Everything happens in **one transaction**: the quote's row lock is taken
    /// first (so a save that raced this send is refused rather than applied
    /// afterwards), then the counter's row lock — the same order every send
    /// takes them in, so concurrent sends queue instead of deadlocking, and a
    /// failure gives the number back.
    ///
    /// The send date is **today according to the database**, read inside the
    /// same transaction rather than taken from the caller; the validity date is
    /// that day plus the days already snapshotted on the document.
    ///
    /// A quote with no lines is refused: an offer that says nothing is a
    /// mistake worth reporting rather than obeying, and it would spend a number.
    ///
    /// This store call does not send an email — the drafting surface (B1.15,
    /// B1.18) does that. It records that the offer was made.
    ///
    /// # Errors
    /// [`StoreError::NotFound`] when the quote is absent or another tenant's;
    /// [`StoreError::Conflict`] when it is not a draft;
    /// [`StoreError::Validation`] when it has no lines; [`StoreError::Db`] on
    /// failure.
    pub async fn send_billing_quote(&self, id: &BillingQuoteId) -> Result<QuoteDocument> {
        let mut tx = self.pool.begin().await.map_err(StoreError::Db)?;
        // The lock also hands back the validity this quote was raised with —
        // never today's default — which the date below is derived from.
        let locked = self.lock_quote(&mut tx, id).await?;
        locked.status.ensure_transition(QuoteStatus::Sent)?;

        // Read under the same lock: whether the document offers anything at all.
        let lines: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM billing_quote_lines WHERE tenant_id = $1 AND quote_id = $2",
        )
        .bind(self.tenant.as_str())
        .bind(id.as_str())
        .fetch_one(&mut *tx)
        .await
        .map_err(StoreError::Db)?;
        if lines == 0 {
            return Err(StoreError::Validation(
                "a quote with no lines cannot be sent; add a line first".to_owned(),
            ));
        }

        // One clock for the whole transaction, and the same clock the row's own
        // timestamps use.
        let today: Date = sqlx::query_scalar("SELECT CURRENT_DATE")
            .fetch_one(&mut *tx)
            .await
            .map_err(StoreError::Db)?;
        let until = today
            .checked_add(Duration::days(i64::from(locked.valid_days)))
            .ok_or_else(|| {
                StoreError::Validation(
                    "the validity puts the expiry date outside the supported range".to_owned(),
                )
            })?;
        let drawn = draw_next(
            &mut tx,
            self.tenant.as_str(),
            QUOTE_SEQUENCE_KIND,
            today.year(),
        )
        .await?;
        let number = document_number(QUOTE_NUMBER_PREFIX, today.year(), drawn);

        sqlx::query(
            "UPDATE billing_quotes \
                SET status = 'sent', number = $3, sent_date = $4, valid_until = $5, \
                    updated_at = now() \
             WHERE tenant_id = $1 AND id = $2",
        )
        .bind(self.tenant.as_str())
        .bind(id.as_str())
        .bind(&number)
        .bind(today)
        .bind(until)
        .execute(&mut *tx)
        .await
        .map_err(StoreError::Db)?;
        tx.commit().await.map_err(StoreError::Db)?;

        self.billing_quote(id).await?.ok_or(StoreError::NotFound)
    }

    /// Writes the closing transition inside `tx`, stamping the day it was
    /// decided. The caller has already taken the quote's lock and checked the
    /// transition.
    ///
    /// One statement for all three closing transitions: they differ only in the
    /// state they write, and writing them through one place is what keeps
    /// `decided_date` and `status` in step (the table's CHECK insists they are,
    /// so a second path would be a second chance to disagree).
    ///
    /// The decision date is the database's `CURRENT_DATE`, read inside the same
    /// transaction as the write and never supplied by the caller.
    async fn write_close(
        &self,
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
        id: &BillingQuoteId,
        to: QuoteStatus,
    ) -> Result<()> {
        sqlx::query(
            "UPDATE billing_quotes SET status = $3, decided_date = CURRENT_DATE, \
                 updated_at = now() \
             WHERE tenant_id = $1 AND id = $2",
        )
        .bind(self.tenant.as_str())
        .bind(id.as_str())
        .bind(to.as_str())
        .execute(&mut **tx)
        .await
        .map_err(StoreError::Db)?;
        Ok(())
    }

    /// Closes an open offer with nothing else to do — declining and expiring.
    /// Acceptance takes its own path, because it also raises a document.
    async fn close_billing_quote(
        &self,
        id: &BillingQuoteId,
        to: QuoteStatus,
    ) -> Result<QuoteDocument> {
        let mut tx = self.pool.begin().await.map_err(StoreError::Db)?;
        self.lock_quote(&mut tx, id)
            .await?
            .status
            .ensure_transition(to)?;
        self.write_close(&mut tx, id, to).await?;
        tx.commit().await.map_err(StoreError::Db)?;

        self.billing_quote(id).await?.ok_or(StoreError::NotFound)
    }

    /// Accepts a **sent** quote — the customer took the offer — and raises the
    /// **draft invoice** for it in the same transaction: the same customer,
    /// currency and customer reference, and a copy of every line, in the
    /// original's order, at the prices the offer was made at.
    ///
    /// **Acceptance and the draft are one act.** Either the offer closes and
    /// its invoice exists, or nothing happened: a quote recorded as accepted
    /// with no document to bill it, or a draft invoice for an offer still shown
    /// as open, would each be a state a user has no way to repair (acceptance
    /// is terminal, so no retry could finish the job). The quote's row lock is
    /// held across the whole thing, so a decline racing this acceptance either
    /// lands first — and the acceptance is refused — or waits.
    ///
    /// The invoice is a **draft**, deliberately. What was offered is what will
    /// be billed, but *when* and *whether it is billed in one go* are the
    /// tenant's decisions; it is issued through the ordinary
    /// [`AccountStore::issue_billing_invoice`], which is what draws the legal
    /// number. Its totals equal the accepted quote's to the cent, because the
    /// lines are the same lines and the arithmetic is the one in
    /// [`crate::billing_totals`].
    ///
    /// A lapsed offer (past its validity, still `sent`) can still be accepted
    /// on purpose — honouring an offer a few days late is a decision the tenant
    /// is entitled to make, and the store refuses on **state**, never on a date
    /// it read. What is copied and what is not is documented on
    /// [`AccountStore::insert_invoice_from_quote`].
    ///
    /// # Errors
    /// [`StoreError::NotFound`] when the quote is absent or another tenant's;
    /// [`StoreError::Conflict`] when it is not an open offer;
    /// [`StoreError::Db`] on failure.
    pub async fn accept_billing_quote(&self, id: &BillingQuoteId) -> Result<QuoteAcceptance> {
        let mut tx = self.pool.begin().await.map_err(StoreError::Db)?;
        let locked = self.lock_quote(&mut tx, id).await?;
        locked.status.ensure_transition(QuoteStatus::Accepted)?;

        // The lines, read under the lock that froze them when the offer was
        // sent. A sent quote always has at least one (an empty one cannot be
        // sent), so neither branch below ever raises a document that says
        // nothing.
        let source = billing_quote_lines::read(&mut *tx, self.tenant.as_str(), id.as_str()).await?;

        // **The routing, and it is decided by the lines rather than by a
        // setting** (ADR 0054 §5): an offer with goods on it becomes a sales
        // order, because goods have to be reserved, picked and delivered before
        // anybody is billed for them. An offer of services has nothing to
        // reserve and nothing to pick, so it goes straight to a draft invoice —
        // exactly as every accepted quote has since B1.12, byte for byte.
        let sells_goods = source.iter().any(|l| l.product_id.is_some());
        let outcome = if sells_goods {
            let order_id = self
                .insert_order_from_quote(&mut tx, id, &locked, &source)
                .await?;
            AcceptedAs::SalesOrder(order_id)
        } else {
            let invoice_id = self
                .insert_invoice_from_quote(
                    &mut tx,
                    &InvoiceFromQuote {
                        quote_id: id.as_str(),
                        customer_id: &locked.customer_id,
                        currency: &locked.currency,
                        reference: &locked.reference,
                    },
                )
                .await?;
            // The copy: the same descriptions, units, quantities, prices and
            // rates, in the same print order.
            for line in &source {
                INVOICE_LINES
                    .write(
                        &mut tx,
                        self.tenant.as_str(),
                        invoice_id.as_str(),
                        line.line.line_order,
                        &line.line.copied(),
                    )
                    .await?;
            }
            // The accepted offer and the invoice draft begin as the same
            // customer-facing document. Copy its appearance in this same
            // transaction so acceptance cannot produce a half-converted pair.
            sqlx::query(
                "INSERT INTO billing_invoice_designs \
                 (tenant_id, invoice_id, design, updated_by) \
                 SELECT tenant_id, $3, design, $4 FROM billing_quote_designs \
                 WHERE tenant_id = $1 AND quote_id = $2",
            )
            .bind(self.tenant.as_str())
            .bind(id.as_str())
            .bind(invoice_id.as_str())
            .bind(self.user.as_str())
            .execute(&mut *tx)
            .await
            .map_err(StoreError::Db)?;
            AcceptedAs::InvoiceDraft(invoice_id)
        };

        self.write_close(&mut tx, id, QuoteStatus::Accepted).await?;
        tx.commit().await.map_err(StoreError::Db)?;

        Ok(QuoteAcceptance {
            quote: self.billing_quote(id).await?.ok_or(StoreError::NotFound)?,
            outcome,
        })
    }

    /// Raises the **draft sales order** an accepted goods quote becomes, inside
    /// the accepting transaction, carrying every line with its product.
    ///
    /// A draft, deliberately, and for the reason the invoice branch is a draft:
    /// what was offered is what will be supplied, but confirming it is a
    /// separate act that draws the order's number, freezes the document and —
    /// since O1.a — refuses to promise goods that cannot exist. Raising a
    /// *confirmed* order here would make an acceptance quietly commit stock.
    ///
    /// The order records the offer it came from (migration 0700), so the two
    /// documents can each answer what became of the other.
    async fn insert_order_from_quote(
        &self,
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
        quote_id: &BillingQuoteId,
        locked: &LockedQuote,
        lines: &[QuoteLine],
    ) -> Result<crate::id::InvSalesOrderId> {
        let order_id = crate::id::InvSalesOrderId::generate();
        sqlx::query(
            "INSERT INTO inv_sales_orders (tenant_id, id, customer_id, status, currency, \
                 reference, note, quote_id, created_by) \
             VALUES ($1, $2, $3, 'draft', $4, $5, '', $6, $7)",
        )
        .bind(self.tenant.as_str())
        .bind(order_id.as_str())
        .bind(&locked.customer_id)
        .bind(&locked.currency)
        .bind(&locked.reference)
        .bind(quote_id.as_str())
        .bind(self.user.as_str())
        .execute(&mut **tx)
        .await
        .map_err(StoreError::Db)?;

        for line in lines {
            sqlx::query(
                "INSERT INTO inv_sales_order_lines (tenant_id, so_id, id, line_order, \
                     description, unit, qty_milli, unit_price_cents, vat_rate_bp, product_id) \
                 VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)",
            )
            .bind(self.tenant.as_str())
            .bind(order_id.as_str())
            .bind(crate::id::BillingLineId::generate().as_str())
            .bind(line.line.line_order)
            .bind(&line.line.description)
            .bind(&line.line.unit)
            .bind(line.line.qty_milli)
            .bind(line.line.unit_price_cents)
            .bind(line.line.vat_rate_bp)
            .bind(line.product_id.as_ref().map(BillingProductId::as_str))
            .execute(&mut **tx)
            .await
            .map_err(StoreError::Db)?;
        }
        Ok(order_id)
    }

    /// Declines a **sent** quote: the customer turned the offer down, or the
    /// tenant withdrew it. Either way the document stands, readable, with the
    /// day it was closed.
    ///
    /// # Errors
    /// As [`AccountStore::accept_billing_quote`].
    pub async fn decline_billing_quote(&self, id: &BillingQuoteId) -> Result<QuoteDocument> {
        self.close_billing_quote(id, QuoteStatus::Declined).await
    }

    /// Marks a **sent** quote as expired: the offer lapsed without an answer.
    ///
    /// Deliberately an explicit act rather than a background sweep. Nothing in
    /// the business changes at midnight on the validity date — what changes is
    /// that somebody decides to stop chasing it, and that decision has a date
    /// worth recording. Until then [`Quote::is_expired`] tells the reader the
    /// offer has lapsed.
    ///
    /// # Errors
    /// As [`AccountStore::accept_billing_quote`].
    pub async fn expire_billing_quote(&self, id: &BillingQuoteId) -> Result<QuoteDocument> {
        self.close_billing_quote(id, QuoteStatus::Expired).await
    }
}

// ---- row types --------------------------------------------------------------

#[derive(sqlx::FromRow)]
struct LockedRow {
    status: String,
    valid_days: i32,
    customer_id: String,
    currency: String,
    reference: String,
}

#[derive(sqlx::FromRow)]
struct QuoteRow {
    id: String,
    customer_id: String,
    status: String,
    currency: String,
    number: Option<String>,
    sent_date: Option<Date>,
    valid_until: Option<Date>,
    valid_days: i32,
    decided_date: Option<Date>,
    reference: String,
    note: String,
    created_by: String,
    created_at: OffsetDateTime,
    updated_at: OffsetDateTime,
}

impl QuoteRow {
    fn into_quote(self) -> Result<Quote> {
        let status = parse_stored_status(&self.status)?;
        Ok(Quote {
            id: BillingQuoteId::new(self.id),
            customer_id: BillingCustomerId::new(self.customer_id),
            status,
            currency: self.currency,
            number: self.number,
            sent_date: self.sent_date,
            valid_until: self.valid_until,
            valid_days: self.valid_days,
            decided_date: self.decided_date,
            reference: self.reference,
            note: self.note,
            created_by: self.created_by,
            created_at: self.created_at,
            updated_at: self.updated_at,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every state, in one place, so a state added later fails these tests
    /// until its rules are stated.
    const ALL: [QuoteStatus; 5] = [
        QuoteStatus::Draft,
        QuoteStatus::Sent,
        QuoteStatus::Accepted,
        QuoteStatus::Declined,
        QuoteStatus::Expired,
    ];

    #[test]
    fn every_status_round_trips_through_its_stored_form() {
        for status in ALL {
            assert_eq!(QuoteStatus::parse(status.as_str()), Some(status));
        }
        assert!(QuoteStatus::Draft.is_draft());
        for other in [
            QuoteStatus::Sent,
            QuoteStatus::Accepted,
            QuoteStatus::Declined,
            QuoteStatus::Expired,
        ] {
            assert!(!other.is_draft());
        }
    }

    #[test]
    fn an_unknown_stored_status_is_never_guessed_at() {
        // Including an invoice's states: the two documents do not share a
        // status vocabulary, and reading one as the other would be a frozen
        // document silently becoming editable.
        for bad in ["", "Draft", "DRAFT", "issued", "paid", "void", "sent "] {
            assert_eq!(QuoteStatus::parse(bad), None, "{bad:?}");
        }
        match parse_stored_status("issued") {
            Err(StoreError::Db(_)) => {}
            other => panic!("expected a decode failure, got {other:?}"),
        }
        assert!(parse_stored_status("draft").is_ok());
    }

    /// The whole lifecycle, stated once as data: exactly these ordered pairs
    /// are legal and the other twenty are not.
    const LEGAL: [(QuoteStatus, QuoteStatus); 4] = [
        (QuoteStatus::Draft, QuoteStatus::Sent),
        (QuoteStatus::Sent, QuoteStatus::Accepted),
        (QuoteStatus::Sent, QuoteStatus::Declined),
        (QuoteStatus::Sent, QuoteStatus::Expired),
    ];

    #[test]
    fn exactly_the_lifecycle_transitions_are_allowed() {
        for from in ALL {
            for to in ALL {
                let expected = LEGAL.contains(&(from, to));
                assert_eq!(
                    from.can_advance_to(to),
                    expected,
                    "{from:?} → {to:?} should be {}",
                    if expected { "allowed" } else { "refused" }
                );
                assert_eq!(from.ensure_transition(to).is_ok(), expected);
            }
        }
    }

    #[test]
    fn no_state_moves_to_itself() {
        // Re-sending would draw a second number; accepting twice would hide a
        // caller that has lost track of the document. Both are refused.
        for status in ALL {
            assert!(!status.can_advance_to(status), "{status:?}");
        }
    }

    #[test]
    fn a_closed_quote_never_reopens() {
        for closed in [
            QuoteStatus::Accepted,
            QuoteStatus::Declined,
            QuoteStatus::Expired,
        ] {
            assert!(closed.is_closed());
            assert!(
                closed.allowed_next().is_empty(),
                "{closed:?} is terminal: the answer to a change of mind is a new quote"
            );
            let message = match closed.ensure_transition(QuoteStatus::Sent) {
                Err(StoreError::Conflict(message)) => message,
                other => panic!("expected Conflict for {closed:?}, got {other:?}"),
            };
            assert!(message.contains(closed.as_str()), "{message}");
            assert!(message.contains("closed"), "{message}");
        }
        for open in [QuoteStatus::Draft, QuoteStatus::Sent] {
            assert!(!open.is_closed());
        }
    }

    #[test]
    fn a_refused_transition_says_what_is_allowed_instead() {
        // The UI corrects itself from the refusal rather than by a second round
        // trip — and the message carries rules, never customer data.
        let message = match QuoteStatus::Draft.ensure_transition(QuoteStatus::Accepted) {
            Err(StoreError::Conflict(message)) => message,
            other => panic!("expected Conflict, got {other:?}"),
        };
        assert!(
            message.contains("accepted") && message.contains("draft"),
            "{message}"
        );
        assert!(
            message.contains("sent"),
            "a draft can only be sent: {message}"
        );

        let message = match QuoteStatus::Sent.ensure_transition(QuoteStatus::Sent) {
            Err(StoreError::Conflict(message)) => message,
            other => panic!("expected Conflict, got {other:?}"),
        };
        for state in ["accepted", "declined", "expired"] {
            assert!(message.contains(state), "{message} should offer {state}");
        }
    }

    #[test]
    fn only_a_draft_may_be_changed() {
        assert!(QuoteStatus::Draft.ensure_editable().is_ok());
        for frozen in [
            QuoteStatus::Sent,
            QuoteStatus::Accepted,
            QuoteStatus::Declined,
            QuoteStatus::Expired,
        ] {
            match frozen.ensure_editable() {
                Err(StoreError::Conflict(message)) => {
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
    fn validity_is_ranged_and_zero_is_an_offer_good_today() {
        for ok in [0, 14, DEFAULT_QUOTE_VALID_DAYS, QUOTE_VALID_MAX_DAYS] {
            assert_eq!(valid_days(ok).unwrap_or(-1), ok);
        }
        for bad in [-1, QUOTE_VALID_MAX_DAYS + 1, i32::MIN, i32::MAX] {
            assert!(
                matches!(valid_days(bad), Err(StoreError::Validation(_))),
                "expected rejection: {bad}"
            );
        }
    }

    /// A header in a given state, for the lapsed predicate: everything else
    /// about a quote is irrelevant to it.
    fn dated(status: QuoteStatus, until: Option<Date>) -> Quote {
        Quote {
            id: BillingQuoteId::new("quo"),
            customer_id: BillingCustomerId::new("cust"),
            status,
            currency: "EUR".to_owned(),
            number: Some("QUO-2026-00001".to_owned()),
            sent_date: until,
            valid_until: until,
            valid_days: 30,
            decided_date: None,
            reference: String::new(),
            note: String::new(),
            created_by: "u".to_owned(),
            created_at: OffsetDateTime::UNIX_EPOCH,
            updated_at: OffsetDateTime::UNIX_EPOCH,
        }
    }

    #[test]
    fn only_an_open_offer_past_its_date_has_lapsed() {
        let today = Date::from_calendar_date(2026, time::Month::August, 6)
            .unwrap_or_else(|e| panic!("{e}"));
        let day_before = today
            .previous_day()
            .unwrap_or_else(|| panic!("no yesterday"));
        let day_after = today.next_day().unwrap_or_else(|| panic!("no tomorrow"));

        assert!(dated(QuoteStatus::Sent, Some(day_before)).is_expired(today));
        // Valid *until* today means the customer has the whole day.
        assert!(!dated(QuoteStatus::Sent, Some(today)).is_expired(today));
        assert!(!dated(QuoteStatus::Sent, Some(day_after)).is_expired(today));
        // No validity date at all (a draft's shape) is never lapsed.
        assert!(!dated(QuoteStatus::Sent, None).is_expired(today));
        // Never offered, or already answered: not an open offer, so not lapsed
        // — including one already moved to `expired`, whose lapse is now a
        // recorded decision rather than a derived flag.
        for other in [
            QuoteStatus::Draft,
            QuoteStatus::Accepted,
            QuoteStatus::Declined,
            QuoteStatus::Expired,
        ] {
            assert!(
                !dated(other, Some(day_before)).is_expired(today),
                "{other:?} is not an open offer"
            );
        }
    }

    #[test]
    fn a_new_quote_defaults_to_the_customers_currency_and_our_validity() {
        let input = NewQuote::for_customer(BillingCustomerId::new("cust"));
        assert!(
            input.currency.is_none(),
            "None means: take the customer's currency"
        );
        assert!(
            input.valid_days.is_none(),
            "None means: the default validity"
        );
        assert!(input.reference.is_empty() && input.note.is_empty());
    }
}
