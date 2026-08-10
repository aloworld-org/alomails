//! Sales orders — what a customer asked us for (alo Inventory, ADR 0035, wave
//! B5.06a), reached through the account door like every other business record.
//!
//! A sales order is the purchase order mirrored ([`crate::inv_po`]): a **draft**
//! that is freely editable and carries no number, and a document we have
//! committed to and frozen. What differs is the counterparty — a customer, not a
//! supplier — and which way the goods go: an order we place ends in goods
//! **arriving**, an order we accept ends in goods **leaving**.
//!
//! **The lifecycle is `draft → confirmed → partially_delivered → delivered`,
//! with `cancelled` reachable from the three states that are not terminal**, and
//! nothing else. Every transition goes through one pure table
//! ([`SoStatus::can_advance_to`]), so the rules are unit-tested over all
//! twenty-five ordered pairs rather than scattered across the write paths.
//! `delivered` and `cancelled` are terminal: an order that has gone out, or that
//! the customer dropped, is not re-opened by editing a status — the answer to
//! "they want more" is another order.
//!
//! Two things this module states about the states, both from
//! `docs/design/inventory.md`:
//!
//! - **Confirming reserves nothing.** It draws the number, stamps the day and
//!   freezes the document ([`crate::inv_so_confirm`]) — and moves no stock. A
//!   sales order is a promise; goods move when they are picked. There is
//!   therefore no reserved quantity anywhere in this module to drift out of step
//!   with the ledger.
//! - **Cancelling a part-delivered order un-delivers nothing.** What has gone
//!   out has gone out; the cancellation closes the remainder, and the customer
//!   is invoiced for what they received. Because that is a decision rather than
//!   a slip, [`SoStatus::ensure_cancellable`] makes the caller say it out loud.
//!
//! Delivering ([`SoStatus::PartiallyDelivered`], [`SoStatus::Delivered`]) is
//! entered by a delivery and never by a person choosing a status: it is the
//! consequence of movements out of stock ([`crate::inv_so_deliver`]).
//!
//! Money is never stored: net, VAT and gross are derived from the lines on every
//! read by [`crate::billing_totals`], so a total can never drift from the lines
//! that justify it. The lines themselves are [`crate::inv_so_lines`]'.
//!
//! Tenancy is structural: every statement carries `tenant_id` from the handle,
//! the customer link and every product a line names are re-checked under the
//! same handle before they are written, and the database backs both with
//! composite foreign keys.

use time::{Date, OffsetDateTime};

use crate::account::AccountStore;
use crate::billing_field::{bounded, currency};
use crate::billing_invoices::{INVOICE_NOTE_MAX_CHARS, INVOICE_REFERENCE_MAX_CHARS};
use crate::billing_line::{FiguresRow, group_figures};
use crate::billing_totals::{LineFigures, Totals, totals};
use crate::error::{Result, StoreError};
use crate::id::{BillingCustomerId, InvSalesOrderId};
use crate::inv_so_lines::{self, NewSoLine, NormalizedSoLine, SoLine};

/// The columns every read of an order selects, in `SoRow` order.
const SO_COLS: &str = "id, customer_id, status, currency, number, confirmed_date, expected_date, \
     closed_date, reference, note, created_by, created_at, updated_at";

/// Where an order is in its life.
///
/// `draft → confirmed → partially_delivered → delivered`, with `cancelled`
/// reachable from every state that is not already terminal.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SoStatus {
    /// Editable, unnumbered, not yet promised to anybody.
    Draft,
    /// Numbered, dated and frozen: we have said yes.
    Confirmed,
    /// Some quantity of some line has gone out. Entered by a delivery, never by
    /// a person.
    PartiallyDelivered,
    /// Every line's delivered quantity equals what was ordered. Terminal.
    Delivered,
    /// The order will not be fulfilled, or not fulfilled further. Terminal.
    Cancelled,
}

impl SoStatus {
    /// The value stored in the `status` column.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Draft => "draft",
            Self::Confirmed => "confirmed",
            Self::PartiallyDelivered => "partially_delivered",
            Self::Delivered => "delivered",
            Self::Cancelled => "cancelled",
        }
    }

    /// Parses a stored status, or `None` if it is not one we know.
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "draft" => Some(Self::Draft),
            "confirmed" => Some(Self::Confirmed),
            "partially_delivered" => Some(Self::PartiallyDelivered),
            "delivered" => Some(Self::Delivered),
            "cancelled" => Some(Self::Cancelled),
            _ => None,
        }
    }

    /// Whether the order is still editable — and therefore also deletable, and
    /// still without a number.
    pub fn is_draft(self) -> bool {
        matches!(self, Self::Draft)
    }

    /// Whether we have committed to the order, and so it carries a number and a
    /// confirmation date. `cancelled` is deliberately not here: an order
    /// cancelled while still a draft was never confirmed, and one cancelled
    /// afterwards keeps the number the customer holds.
    pub fn is_committed(self) -> bool {
        matches!(
            self,
            Self::Confirmed | Self::PartiallyDelivered | Self::Delivered
        )
    }

    /// Whether the order is finished with, one way or the other — and therefore
    /// stamped with the day it closed.
    pub fn is_closed(self) -> bool {
        matches!(self, Self::Delivered | Self::Cancelled)
    }

    /// Whether goods may still go out against this order — what the shortage
    /// report (B5.07) means by "promised".
    pub fn is_open(self) -> bool {
        matches!(self, Self::Confirmed | Self::PartiallyDelivered)
    }

    /// The states this one may move to. **The whole lifecycle is this
    /// function**: a draft is confirmed, a confirmed order goes out in part or
    /// in full, a partly-delivered one completes, and anything not yet finished
    /// can be given up on.
    pub fn allowed_next(self) -> &'static [Self] {
        match self {
            Self::Draft => &[Self::Confirmed, Self::Cancelled],
            Self::Confirmed => &[Self::PartiallyDelivered, Self::Delivered, Self::Cancelled],
            Self::PartiallyDelivered => &[Self::Delivered, Self::Cancelled],
            Self::Delivered | Self::Cancelled => &[],
        }
    }

    /// Whether `to` is a legal move from this state. Never true for `to ==
    /// self`: re-confirming an order would draw a second number, and cancelling
    /// twice is a caller that has lost track of the document, which answering
    /// "fine" would hide.
    ///
    /// A second **delivery** against a partly-delivered order is not a repeated
    /// transition — it is a new delivery that leaves the order in the same
    /// state, and [`crate::inv_so_deliver`] writes it without asking this table.
    pub fn can_advance_to(self, to: Self) -> bool {
        self.allowed_next().contains(&to)
    }

    /// The guard every transition runs.
    ///
    /// The refusal names both states **and** what this state does allow, so a UI
    /// can correct itself without a second round trip.
    ///
    /// # Errors
    /// [`StoreError::Conflict`] (`409` at the route edge) when the move is not
    /// in [`SoStatus::allowed_next`].
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
            "a sales order cannot become {} while it is {}; {allowance}",
            to.as_str(),
            self.as_str()
        )))
    }

    /// The guard every write path runs before it changes an order's content: a
    /// draft may be edited and deleted, anything else is frozen.
    ///
    /// Changing a line after the customer has our confirmation would make our
    /// copy disagree with theirs, and the correction for that is a new order.
    ///
    /// # Errors
    /// [`StoreError::Conflict`] naming the status that refused (`409`).
    pub fn ensure_editable(self) -> Result<()> {
        if self.is_draft() {
            return Ok(());
        }
        Err(StoreError::Conflict(format!(
            "a sales order can only be changed while it is a draft; this one is {}",
            self.as_str()
        )))
    }

    /// The guard delivering runs before it moves a single unit: goods only leave
    /// against an order we have actually confirmed.
    ///
    /// A **draft** is refused because nobody has promised anything, a
    /// **delivered** one because there is nothing left to send, and a
    /// **cancelled** one because the promise was withdrawn — a van loaded
    /// against any of the three is a conversation, not a booking, and stock
    /// leaving silently is how a shortage report starts lying.
    ///
    /// A second delivery against a `partially_delivered` order is ordinary and
    /// is **not** a transition ([`SoStatus::can_advance_to`] never allows a
    /// state to itself), which is why this guard exists beside the table rather
    /// than inside it.
    ///
    /// # Errors
    /// [`StoreError::Conflict`] (`409` at the route edge) naming the state that
    /// refused and what to do about it.
    pub fn ensure_deliverable(self) -> Result<()> {
        if self.is_open() {
            return Ok(());
        }
        let because = match self {
            Self::Draft => "it has not been confirmed with the customer yet",
            Self::Delivered => "everything on it has already gone out",
            _ => "it was cancelled",
        };
        Err(StoreError::Conflict(format!(
            "goods cannot be delivered against a sales order that is {}: {because}",
            self.as_str()
        )))
    }

    /// The guard invoicing runs ([`crate::inv_so_invoice`]) before it raises a
    /// document for what has gone out.
    ///
    /// Only a **draft** is refused, and only because nobody has promised
    /// anything yet: an order still being typed has moved no goods, and billing
    /// a conversation is how a customer receives an invoice for a quote they
    /// never accepted. Every other state may be invoiced — including
    /// `cancelled`, which is the case the cancellation itself names out loud:
    /// giving up on a part-delivered order closes the remainder and **leaves
    /// the customer to be invoiced for what they received**.
    ///
    /// Whether there is anything left to bill is a separate question, answered
    /// from the lines rather than from the state.
    ///
    /// # Errors
    /// [`StoreError::Conflict`] (`409` at the route edge) when the order is
    /// still a draft.
    pub fn ensure_invoiceable(self) -> Result<()> {
        if !self.is_draft() {
            return Ok(());
        }
        Err(StoreError::Conflict(
            "this sales order is still a draft: nothing has been promised and nothing has gone \
             out, so there is nothing to invoice"
                .to_owned(),
        ))
    }

    /// The guard cancelling runs **in addition** to the transition table:
    /// giving up on an order some of whose goods have already gone out closes
    /// the remainder for good, and the customer is invoiced for what they
    /// received — so it has to be said out loud.
    ///
    /// `short_close` is the caller's explicit "yes, that is all they are
    /// getting". It is ignored from every other state, where cancelling needs no
    /// such admission.
    ///
    /// # Errors
    /// [`StoreError::Conflict`] when the order is partly delivered and the
    /// caller has not accepted closing the remainder.
    pub fn ensure_cancellable(self, short_close: bool) -> Result<()> {
        self.ensure_transition(Self::Cancelled)?;
        if matches!(self, Self::PartiallyDelivered) && !short_close {
            return Err(StoreError::Conflict(
                "part of this order has already gone out; cancelling it closes the remainder for \
                 good and leaves the customer to be invoiced for what they received, which has to \
                 be asked for explicitly"
                    .to_owned(),
            ));
        }
        Ok(())
    }
}

/// Turns a stored status string into a status, or reports corrupt data.
///
/// A status the code does not know is corrupt data, not user input: it is
/// reported as a decode failure (detail in the source, never in the message)
/// rather than guessed at, because guessing here would mean treating a frozen
/// document as editable.
fn parse_stored_status(stored: &str) -> Result<SoStatus> {
    SoStatus::parse(stored).ok_or_else(|| {
        StoreError::Db(sqlx::Error::Decode(
            "inv_sales_orders.status is not a known status".into(),
        ))
    })
}

/// The writable header of an order, used for both create and update (an update
/// is a full replace — the route layer merges a partial `PATCH` onto the stored
/// record before calling). Lines are written separately, as a set.
///
/// `currency` is `None` to mean *take the customer's*. Whatever is resolved is
/// **stored on the document**: a customer who is re-denominated next year must
/// not restate an order taken this year.
#[derive(Debug, Clone)]
pub struct NewSalesOrder {
    /// Who we are selling to. Must be one of this tenant's customers.
    pub customer_id: BillingCustomerId,
    /// ISO 4217 code, or `None` for the customer's default.
    pub currency: Option<String>,
    /// The day we promised the goods, or `None` while nobody has said.
    pub expected_date: Option<Date>,
    /// The customer's own reference for the order — their PO number, a project
    /// code — printed on the document.
    pub reference: String,
    /// Free-text note printed under the lines.
    pub note: String,
}

impl NewSalesOrder {
    /// The blank header a new draft starts from: this customer, their currency,
    /// no promised date, no reference and no note. There is deliberately no
    /// [`Default`] — an order without a customer is not a document.
    pub fn for_customer(customer_id: BillingCustomerId) -> Self {
        Self {
            customer_id,
            currency: None,
            expected_date: None,
            reference: String::new(),
            note: String::new(),
        }
    }
}

/// The header of a stored order. Its money lives in [`Totals`], computed from
/// the lines.
#[derive(Debug, Clone)]
pub struct SalesOrder {
    /// Opaque id, unique within the tenant.
    pub id: InvSalesOrderId,
    /// Who we are selling to.
    pub customer_id: BillingCustomerId,
    /// Where the order is in its life.
    pub status: SoStatus,
    /// ISO 4217 code the order was taken in.
    pub currency: String,
    /// The document number, `None` until the order is confirmed.
    pub number: Option<String>,
    /// The day we said yes, `None` until the order is confirmed.
    pub confirmed_date: Option<Date>,
    /// The day the goods were promised, `None` while nobody has said.
    pub expected_date: Option<Date>,
    /// The day the order was delivered in full or cancelled; `None` while it is
    /// still open (or was never confirmed).
    pub closed_date: Option<Date>,
    /// The customer's own reference.
    pub reference: String,
    /// Free-text note.
    pub note: String,
    /// The user who took the order.
    pub created_by: String,
    /// Creation time.
    pub created_at: OffsetDateTime,
    /// Last modification time — moved by a header edit, a line edit and every
    /// transition, since all three change what the document says.
    pub updated_at: OffsetDateTime,
}

impl SalesOrder {
    /// Whether the goods are **late**: the order is still open and we promised
    /// them before `today`.
    ///
    /// Derived, never stored — a stored flag would be wrong every midnight, the
    /// same rule an invoice's overdue flag and a purchase order's late flag
    /// follow. An order nobody has confirmed, and one that is finished with, are
    /// never late whatever the dates say.
    pub fn is_late(&self, today: Date) -> bool {
        self.status.is_open() && self.expected_date.is_some_and(|when| when < today)
    }
}

/// An order as a list entry: the header, who it is for, and what it is worth —
/// without the lines. The totals are computed, never read from a column.
#[derive(Debug, Clone)]
pub struct SalesOrderSummary {
    /// The header.
    pub order: SalesOrder,
    /// The customer's name as it stands now. An id is not an explanation, and
    /// this list is read by a person.
    pub customer_name: String,
    /// Net, VAT breakdown and gross, derived from the lines.
    pub totals: Totals,
}

/// A whole order: header, customer name, lines in print order, and the totals
/// derived from those lines.
#[derive(Debug, Clone)]
pub struct SalesOrderDocument {
    /// The header.
    pub order: SalesOrder,
    /// The customer's name as it stands now.
    pub customer_name: String,
    /// The lines, in print order.
    pub lines: Vec<SoLine>,
    /// Net, VAT breakdown and gross, derived from `lines`.
    pub totals: Totals,
}

/// The header, validated and with the customer's defaults resolved.
#[derive(Debug)]
struct NormalizedSo {
    customer_id: String,
    currency: String,
    expected_date: Option<Date>,
    reference: String,
    note: String,
}

impl AccountStore {
    /// Resolves a header against **this tenant's** customer: the customer must
    /// exist under this handle, so a guessed id from another tenant is a
    /// `NotFound`, and it must be active — archiving a customer means "we no
    /// longer trade with them", so taking an order from them is a mistake worth
    /// reporting rather than obeying.
    ///
    /// The promised date is taken as given, including one in the past: an order
    /// typed up from a paper one taken last week is an ordinary thing to do, and
    /// refusing it would only teach people to lie about the date.
    async fn normalize_sales_order(&self, input: &NewSalesOrder) -> Result<NormalizedSo> {
        let customer = self
            .billing_customer(&input.customer_id)
            .await?
            .ok_or(StoreError::NotFound)?;
        if customer.is_archived() {
            return Err(StoreError::Validation(
                "the customer is archived; restore them before taking orders from them again"
                    .to_owned(),
            ));
        }
        let resolved_currency = match input.currency.as_deref() {
            Some(code) => currency(code)?,
            None => customer.currency,
        };
        Ok(NormalizedSo {
            customer_id: customer.id.as_str().to_owned(),
            currency: resolved_currency,
            expected_date: input.expected_date,
            reference: bounded("reference", &input.reference, INVOICE_REFERENCE_MAX_CHARS)?,
            note: bounded("note", &input.note, INVOICE_NOTE_MAX_CHARS)?,
        })
    }

    /// Validates a line set and holds every product it names to **this tenant's**
    /// catalog.
    ///
    /// A product that is not ours is a `NotFound` — existence is never disclosed
    /// — and an archived one is a `Validation` naming the line, since archiving
    /// a product means the tenant has stopped carrying it and promising more is
    /// a mistake worth reporting. Both are decided before a single row is
    /// written.
    async fn normalize_so_lines(&self, lines: &[NewSoLine]) -> Result<Vec<NormalizedSoLine>> {
        let normalized = inv_so_lines::normalize_so_lines(lines)?;
        let named = inv_so_lines::products_named(&normalized);
        if named.is_empty() {
            return Ok(normalized);
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

        for (index, line) in normalized.iter().enumerate() {
            let Some(product_id) = line.product_id.as_deref() else {
                continue;
            };
            let archived = rows
                .iter()
                .find(|(id, _)| id == product_id)
                .map(|(_, archived)| *archived)
                .ok_or(StoreError::NotFound)?;
            if archived {
                return Err(StoreError::Validation(format!(
                    "line {}: that product is archived; restore it in the catalog before \
                     promising more of it",
                    index + 1
                )));
            }
        }
        Ok(normalized)
    }

    /// Takes the order's row lock inside `tx` and returns its status, so a
    /// caller can check whether it may write and then write without any other
    /// transaction slipping in between. Two writers to one order serialise here;
    /// a writer that arrives after a confirmation sees `confirmed`.
    ///
    /// # Errors
    /// [`StoreError::NotFound`] when the id is absent **or another tenant's**;
    /// [`StoreError::Db`] on failure or on a status the code does not know.
    async fn lock_sales_order(
        &self,
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
        id: &InvSalesOrderId,
    ) -> Result<SoStatus> {
        let stored: Option<String> = sqlx::query_scalar(
            "SELECT status FROM inv_sales_orders WHERE tenant_id = $1 AND id = $2 FOR UPDATE",
        )
        .bind(self.tenant.as_str())
        .bind(id.as_str())
        .fetch_optional(&mut **tx)
        .await
        .map_err(StoreError::Db)?;
        parse_stored_status(&stored.ok_or(StoreError::NotFound)?)
    }

    /// The status of one of this tenant's orders, without taking a lock — the
    /// cheap pre-check that lets a write refuse a frozen document before it does
    /// any other work, and that answers `NotFound` for another tenant's id
    /// **before** anything else about the request is judged. It is never the
    /// authority: every write re-reads the status under
    /// [`AccountStore::lock_sales_order`] before writing.
    ///
    /// `pub(crate)` for delivering ([`crate::inv_so_deliver`]), which has the
    /// same two reasons to ask and one more: resolving the place goods were
    /// picked from must not produce a refusal about a document the caller is not
    /// allowed to know exists.
    ///
    /// # Errors
    /// [`StoreError::NotFound`] when the id is absent or another tenant's;
    /// [`StoreError::Db`] on failure.
    pub(crate) async fn sales_order_status(&self, id: &InvSalesOrderId) -> Result<SoStatus> {
        let stored: Option<String> = sqlx::query_scalar(
            "SELECT status FROM inv_sales_orders WHERE tenant_id = $1 AND id = $2",
        )
        .bind(self.tenant.as_str())
        .bind(id.as_str())
        .fetch_optional(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        parse_stored_status(&stored.ok_or(StoreError::NotFound)?)
    }

    /// Creates a **draft** order with no lines — the state a new order starts
    /// in. It carries no number and no confirmation date by construction; only
    /// confirming assigns those.
    ///
    /// # Errors
    /// [`StoreError::NotFound`] when the customer is not this tenant's;
    /// [`StoreError::Validation`] when the customer is archived or a header
    /// field breaks its rule; [`StoreError::Db`] on failure.
    pub async fn create_inv_sales_order(&self, input: &NewSalesOrder) -> Result<InvSalesOrderId> {
        let header = self.normalize_sales_order(input).await?;
        let id = InvSalesOrderId::generate();
        sqlx::query(
            "INSERT INTO inv_sales_orders (tenant_id, id, customer_id, status, currency, \
                 expected_date, reference, note, created_by) \
             VALUES ($1, $2, $3, 'draft', $4, $5, $6, $7, $8)",
        )
        .bind(self.tenant.as_str())
        .bind(id.as_str())
        .bind(&header.customer_id)
        .bind(&header.currency)
        .bind(header.expected_date)
        .bind(&header.reference)
        .bind(&header.note)
        .bind(self.user.as_str())
        .execute(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        Ok(id)
    }

    /// The tenant's orders, newest first, each with its customer's name and its
    /// computed totals. `status` filters; `None` lists everything.
    ///
    /// The lines of every listed order are fetched in one further statement, not
    /// one per order.
    ///
    /// # Errors
    /// [`StoreError::Db`] on failure.
    pub async fn inv_sales_orders(
        &self,
        status: Option<SoStatus>,
    ) -> Result<Vec<SalesOrderSummary>> {
        let status = status.map(SoStatus::as_str);
        let rows = sqlx::query_as::<_, SoRow>(&format!(
            "SELECT {SO_COLS}, \
                 (SELECT name FROM billing_customers c \
                   WHERE c.tenant_id = o.tenant_id AND c.id = o.customer_id) AS customer_name \
             FROM inv_sales_orders o \
             WHERE tenant_id = $1 AND ($2::text IS NULL OR status = $2) \
             ORDER BY created_at DESC, id"
        ))
        .bind(self.tenant.as_str())
        .bind(status)
        .fetch_all(&self.pool)
        .await
        .map_err(StoreError::Db)?;

        let figures = sqlx::query_as::<_, FiguresRow>(
            "SELECT so_id AS doc_id, qty_milli, unit_price_cents, vat_rate_bp \
             FROM inv_sales_order_lines \
             WHERE tenant_id = $1 AND so_id IN ( \
                 SELECT id FROM inv_sales_orders \
                 WHERE tenant_id = $1 AND ($2::text IS NULL OR status = $2))",
        )
        .bind(self.tenant.as_str())
        .bind(status)
        .fetch_all(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        let mut by_order = group_figures(figures);

        rows.into_iter()
            .map(|row| {
                let lines = by_order.remove(&row.id).unwrap_or_default();
                let customer_name = row.customer_name.clone().unwrap_or_default();
                Ok(SalesOrderSummary {
                    order: row.into_order()?,
                    customer_name,
                    totals: totals(&lines),
                })
            })
            .collect()
    }

    /// One order of the tenant with its lines and totals, or `None` — including
    /// when the id belongs to another tenant (indistinguishable by design).
    ///
    /// # Errors
    /// [`StoreError::Db`] on failure.
    pub async fn inv_sales_order(
        &self,
        id: &InvSalesOrderId,
    ) -> Result<Option<SalesOrderDocument>> {
        let Some(row) = sqlx::query_as::<_, SoRow>(&format!(
            "SELECT {SO_COLS}, \
                 (SELECT name FROM billing_customers c \
                   WHERE c.tenant_id = o.tenant_id AND c.id = o.customer_id) AS customer_name \
             FROM inv_sales_orders o WHERE tenant_id = $1 AND id = $2"
        ))
        .bind(self.tenant.as_str())
        .bind(id.as_str())
        .fetch_optional(&self.pool)
        .await
        .map_err(StoreError::Db)?
        else {
            return Ok(None);
        };
        let customer_name = row.customer_name.clone().unwrap_or_default();
        let lines = inv_so_lines::read(&self.pool, self.tenant.as_str(), id.as_str()).await?;
        let figures: Vec<LineFigures> = lines.iter().map(SoLine::figures).collect();
        Ok(Some(SalesOrderDocument {
            order: row.into_order()?,
            customer_name,
            lines,
            totals: totals(&figures),
        }))
    }

    /// The id of the tenant's order **numbered** `number`, or `None`.
    ///
    /// The counterpart of [`AccountStore::inv_purchase_order_id_by_number`], and
    /// for the same reason: a person — and, from B5.10, the inventory agent —
    /// says "SO-2026-00001", never an opaque id. Case-insensitive, blanks
    /// trimmed, otherwise exact; a draft has no number and is therefore
    /// unreachable here; another tenant's number is `None`.
    ///
    /// # Errors
    /// [`StoreError::Db`] on failure.
    pub async fn inv_sales_order_id_by_number(
        &self,
        number: &str,
    ) -> Result<Option<InvSalesOrderId>> {
        let wanted = number.trim();
        if wanted.is_empty() {
            return Ok(None);
        }
        let id: Option<String> = sqlx::query_scalar(
            "SELECT id FROM inv_sales_orders WHERE tenant_id = $1 AND upper(number) = upper($2)",
        )
        .bind(self.tenant.as_str())
        .bind(wanted)
        .fetch_optional(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        Ok(id.map(InvSalesOrderId::new))
    }

    /// Replaces the writable header of a **draft** order: customer, currency,
    /// promised date, reference and note. Status, number and the two stamped
    /// dates are not writable here — they move only through the lifecycle
    /// actions.
    ///
    /// The status is checked before the header is even validated, so a frozen
    /// document is told it is frozen rather than being handed a complaint about
    /// a field it was never going to accept; it is then re-checked under the row
    /// lock that the write itself takes.
    ///
    /// # Errors
    /// [`StoreError::NotFound`] when the order or the customer is not this
    /// tenant's; [`StoreError::Conflict`] when the order is no longer a draft;
    /// [`StoreError::Validation`] as for create; [`StoreError::Db`] on failure.
    pub async fn update_inv_sales_order(
        &self,
        id: &InvSalesOrderId,
        input: &NewSalesOrder,
    ) -> Result<()> {
        self.sales_order_status(id).await?.ensure_editable()?;
        let header = self.normalize_sales_order(input).await?;

        let mut tx = self.pool.begin().await.map_err(StoreError::Db)?;
        // Authoritative: the state that matters is the one under the lock the
        // UPDATE below writes through, not the one read a moment ago. Dropping
        // the transaction on any error rolls it back untouched.
        self.lock_sales_order(&mut tx, id)
            .await?
            .ensure_editable()?;
        sqlx::query(
            "UPDATE inv_sales_orders SET customer_id = $3, currency = $4, expected_date = $5, \
                 reference = $6, note = $7, updated_at = now() \
             WHERE tenant_id = $1 AND id = $2",
        )
        .bind(self.tenant.as_str())
        .bind(id.as_str())
        .bind(&header.customer_id)
        .bind(&header.currency)
        .bind(header.expected_date)
        .bind(&header.reference)
        .bind(&header.note)
        .execute(&mut *tx)
        .await
        .map_err(StoreError::Db)?;
        tx.commit().await.map_err(StoreError::Db)?;
        Ok(())
    }

    /// Replaces the whole line set of a **draft** order, in the caller's order,
    /// in one transaction: either the document reads exactly as the caller sent
    /// it or it is untouched. Line positions are assigned 0-based from that
    /// order, so what was sent is what prints.
    ///
    /// Every line is validated, and every product it names held to this tenant's
    /// catalog, before anything is written; the draft-only guard runs before
    /// even that, under the same lock the replacement writes through — so a set
    /// cannot land on an order that was confirmed while it was being composed.
    ///
    /// # Errors
    /// [`StoreError::NotFound`] when the order, or a product a line names, is
    /// not this tenant's; [`StoreError::Conflict`] when the order is no longer a
    /// draft; [`StoreError::Validation`] when the set is too long or a line
    /// breaks a field rule (the message names the line's position);
    /// [`StoreError::Db`] on failure.
    pub async fn set_inv_sales_order_lines(
        &self,
        id: &InvSalesOrderId,
        lines: &[NewSoLine],
    ) -> Result<()> {
        self.sales_order_status(id).await?.ensure_editable()?;
        let lines = self.normalize_so_lines(lines).await?;

        let mut tx = self.pool.begin().await.map_err(StoreError::Db)?;
        self.lock_sales_order(&mut tx, id)
            .await?
            .ensure_editable()?;
        inv_so_lines::replace(&mut tx, self.tenant.as_str(), id.as_str(), &lines).await?;
        sqlx::query(
            "UPDATE inv_sales_orders SET updated_at = now() WHERE tenant_id = $1 AND id = $2",
        )
        .bind(self.tenant.as_str())
        .bind(id.as_str())
        .execute(&mut *tx)
        .await
        .map_err(StoreError::Db)?;
        tx.commit().await.map_err(StoreError::Db)?;
        Ok(())
    }

    /// Deletes a **draft** order and, by cascade, its lines. This is the only
    /// order that is ever removed: a draft never consumed a number and was never
    /// promised to anybody. One that has been confirmed is cancelled instead,
    /// keeping its number and its content readable.
    ///
    /// # Errors
    /// [`StoreError::NotFound`] when the order is absent or another tenant's;
    /// [`StoreError::Conflict`] when it is no longer a draft;
    /// [`StoreError::Db`] on failure.
    pub async fn delete_inv_sales_order(&self, id: &InvSalesOrderId) -> Result<()> {
        let mut tx = self.pool.begin().await.map_err(StoreError::Db)?;
        self.lock_sales_order(&mut tx, id)
            .await?
            .ensure_editable()?;
        sqlx::query("DELETE FROM inv_sales_orders WHERE tenant_id = $1 AND id = $2")
            .bind(self.tenant.as_str())
            .bind(id.as_str())
            .execute(&mut *tx)
            .await
            .map_err(StoreError::Db)?;
        tx.commit().await.map_err(StoreError::Db)?;
        Ok(())
    }

    /// Cancels an order: it will not be fulfilled, or not fulfilled further.
    ///
    /// Legal from `draft` (an order nobody confirmed, kept rather than deleted
    /// so the decision is on the record), from `confirmed`, and from
    /// `partially_delivered` only when the caller passes `short_close` — that
    /// closes the remainder for good and leaves the customer to be invoiced for
    /// what they received, and it must be asked for rather than slipped into.
    ///
    /// **Nothing is un-delivered.** What has gone out has gone out; the ledger
    /// is append-only and a cancellation writes no movement. Goods that come
    /// back are a return, which is a movement of its own with a person's reason
    /// on it.
    ///
    /// The closing date is the database's `CURRENT_DATE`, read inside the same
    /// transaction as the write and never supplied by the caller. A cancelled
    /// order keeps whatever number it had.
    ///
    /// This store call sends no email. Telling the customer is a letter the
    /// tenant writes, and it goes through the one audited submission path like
    /// every other message this product composes.
    ///
    /// # Errors
    /// [`StoreError::NotFound`] when the order is absent or another tenant's;
    /// [`StoreError::Conflict`] when it is already closed, or partly delivered
    /// without `short_close`; [`StoreError::Db`] on failure.
    pub async fn cancel_inv_sales_order(
        &self,
        id: &InvSalesOrderId,
        short_close: bool,
    ) -> Result<SalesOrderDocument> {
        let mut tx = self.pool.begin().await.map_err(StoreError::Db)?;
        self.lock_sales_order(&mut tx, id)
            .await?
            .ensure_cancellable(short_close)?;
        sqlx::query(
            "UPDATE inv_sales_orders SET status = 'cancelled', closed_date = CURRENT_DATE, \
                 updated_at = now() \
             WHERE tenant_id = $1 AND id = $2",
        )
        .bind(self.tenant.as_str())
        .bind(id.as_str())
        .execute(&mut *tx)
        .await
        .map_err(StoreError::Db)?;
        tx.commit().await.map_err(StoreError::Db)?;

        self.inv_sales_order(id).await?.ok_or(StoreError::NotFound)
    }
}

// ---- row types --------------------------------------------------------------

#[derive(sqlx::FromRow)]
struct SoRow {
    id: String,
    customer_id: String,
    status: String,
    currency: String,
    number: Option<String>,
    confirmed_date: Option<Date>,
    expected_date: Option<Date>,
    closed_date: Option<Date>,
    reference: String,
    note: String,
    created_by: String,
    created_at: OffsetDateTime,
    updated_at: OffsetDateTime,
    /// Joined, not stored: the customer's current name. `None` cannot happen
    /// while the foreign key holds — it is treated as an empty name rather than
    /// a failure, because a list must not disappear over a display string.
    customer_name: Option<String>,
}

impl SoRow {
    fn into_order(self) -> Result<SalesOrder> {
        let status = parse_stored_status(&self.status)?;
        Ok(SalesOrder {
            id: InvSalesOrderId::new(self.id),
            customer_id: BillingCustomerId::new(self.customer_id),
            status,
            currency: self.currency,
            number: self.number,
            confirmed_date: self.confirmed_date,
            expected_date: self.expected_date,
            closed_date: self.closed_date,
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

    /// Every state, in one place, so a state added later fails these tests until
    /// its rules are stated.
    const ALL: [SoStatus; 5] = [
        SoStatus::Draft,
        SoStatus::Confirmed,
        SoStatus::PartiallyDelivered,
        SoStatus::Delivered,
        SoStatus::Cancelled,
    ];

    #[test]
    fn every_status_round_trips_through_its_stored_form() {
        for status in ALL {
            assert_eq!(SoStatus::parse(status.as_str()), Some(status));
        }
        assert!(SoStatus::Draft.is_draft());
        for other in [
            SoStatus::Confirmed,
            SoStatus::PartiallyDelivered,
            SoStatus::Delivered,
            SoStatus::Cancelled,
        ] {
            assert!(!other.is_draft());
        }
    }

    #[test]
    fn an_unknown_stored_status_is_never_guessed_at() {
        // Including a purchase order's, a quote's and an invoice's states: the
        // documents do not share a status vocabulary, and reading one as another
        // would be a frozen document silently becoming editable.
        for bad in [
            "",
            "Draft",
            "DRAFT",
            "sent",
            "received",
            "partially_received",
            "accepted",
            "partially delivered",
            "delivered ",
        ] {
            assert_eq!(SoStatus::parse(bad), None, "{bad:?}");
        }
        match parse_stored_status("received") {
            Err(StoreError::Db(_)) => {}
            other => panic!("expected a decode failure, got {other:?}"),
        }
        assert!(parse_stored_status("partially_delivered").is_ok());
    }

    /// The whole lifecycle, stated once as data: exactly these ordered pairs are
    /// legal and the other eighteen are not.
    const LEGAL: [(SoStatus, SoStatus); 7] = [
        (SoStatus::Draft, SoStatus::Confirmed),
        (SoStatus::Draft, SoStatus::Cancelled),
        (SoStatus::Confirmed, SoStatus::PartiallyDelivered),
        (SoStatus::Confirmed, SoStatus::Delivered),
        (SoStatus::Confirmed, SoStatus::Cancelled),
        (SoStatus::PartiallyDelivered, SoStatus::Delivered),
        (SoStatus::PartiallyDelivered, SoStatus::Cancelled),
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
        // Re-confirming would draw a second number; cancelling twice is a caller
        // that has lost track of the document. A second delivery against a
        // partly-delivered order is not this — it is a delivery, not a
        // transition.
        for status in ALL {
            assert!(!status.can_advance_to(status), "{status:?}");
        }
    }

    #[test]
    fn a_finished_order_never_reopens() {
        for closed in [SoStatus::Delivered, SoStatus::Cancelled] {
            assert!(closed.is_closed());
            assert!(
                closed.allowed_next().is_empty(),
                "{closed:?} is terminal: the answer to wanting more is another order"
            );
            let message = match closed.ensure_transition(SoStatus::Confirmed) {
                Err(StoreError::Conflict(message)) => message,
                other => panic!("expected Conflict for {closed:?}, got {other:?}"),
            };
            assert!(message.contains(closed.as_str()), "{message}");
            assert!(message.contains("closed"), "{message}");
        }
        for unfinished in [
            SoStatus::Draft,
            SoStatus::Confirmed,
            SoStatus::PartiallyDelivered,
        ] {
            assert!(!unfinished.is_closed());
        }
    }

    #[test]
    fn only_a_committed_order_carries_a_number_and_only_an_open_one_owes_goods() {
        // The two predicates the database's CHECKs mirror, and the one the
        // shortage report will ask.
        for committed in [
            SoStatus::Confirmed,
            SoStatus::PartiallyDelivered,
            SoStatus::Delivered,
        ] {
            assert!(committed.is_committed(), "{committed:?}");
        }
        for uncommitted in [SoStatus::Draft, SoStatus::Cancelled] {
            assert!(!uncommitted.is_committed(), "{uncommitted:?}");
        }
        for open in [SoStatus::Confirmed, SoStatus::PartiallyDelivered] {
            assert!(open.is_open(), "{open:?}");
        }
        for shut in [SoStatus::Draft, SoStatus::Delivered, SoStatus::Cancelled] {
            assert!(!shut.is_open(), "{shut:?}");
        }
    }

    #[test]
    fn a_refused_transition_says_what_is_allowed_instead() {
        // The UI corrects itself from the refusal rather than by a second round
        // trip — and the message carries rules, never customer data.
        let message = match SoStatus::Draft.ensure_transition(SoStatus::Delivered) {
            Err(StoreError::Conflict(message)) => message,
            other => panic!("expected Conflict, got {other:?}"),
        };
        assert!(
            message.contains("delivered") && message.contains("draft"),
            "{message}"
        );
        assert!(
            message.contains("confirmed"),
            "a draft can be confirmed: {message}"
        );
        assert!(message.contains("cancelled"), "or given up on: {message}");
    }

    #[test]
    fn only_a_draft_may_be_changed() {
        assert!(SoStatus::Draft.ensure_editable().is_ok());
        for frozen in [
            SoStatus::Confirmed,
            SoStatus::PartiallyDelivered,
            SoStatus::Delivered,
            SoStatus::Cancelled,
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
    fn goods_leave_only_against_an_order_we_confirmed() {
        for open in [SoStatus::Confirmed, SoStatus::PartiallyDelivered] {
            assert!(open.ensure_deliverable().is_ok(), "{open:?}");
        }
        for (shut, because) in [
            (SoStatus::Draft, "not been confirmed"),
            (SoStatus::Delivered, "already gone out"),
            (SoStatus::Cancelled, "cancelled"),
        ] {
            let message = match shut.ensure_deliverable() {
                Err(StoreError::Conflict(message)) => message,
                other => panic!("expected Conflict for {shut:?}, got {other:?}"),
            };
            assert!(message.contains(shut.as_str()), "{message}");
            assert!(message.contains(because), "{message}");
        }
    }

    #[test]
    fn only_an_order_nobody_confirmed_is_refused_an_invoice() {
        // Including `cancelled`: giving up on a part-delivered order closes the
        // remainder and leaves the customer to be invoiced for what they
        // received, which is exactly what the cancellation says out loud.
        for billable in [
            SoStatus::Confirmed,
            SoStatus::PartiallyDelivered,
            SoStatus::Delivered,
            SoStatus::Cancelled,
        ] {
            assert!(billable.ensure_invoiceable().is_ok(), "{billable:?}");
        }
        let message = match SoStatus::Draft.ensure_invoiceable() {
            Err(StoreError::Conflict(message)) => message,
            other => panic!("expected Conflict, got {other:?}"),
        };
        assert!(message.contains("draft"), "{message}");
        assert!(message.contains("nothing to invoice"), "{message}");
    }

    #[test]
    fn giving_up_on_a_part_delivered_order_has_to_be_said_out_loud() {
        // Cancelling what has already partly gone out closes the remainder for
        // good — a decision, not a slip — and un-delivers nothing.
        let message = match SoStatus::PartiallyDelivered.ensure_cancellable(false) {
            Err(StoreError::Conflict(message)) => message,
            other => panic!("expected Conflict, got {other:?}"),
        };
        assert!(message.contains("already gone out"), "{message}");
        assert!(message.contains("invoiced"), "{message}");
        assert!(
            SoStatus::PartiallyDelivered
                .ensure_cancellable(true)
                .is_ok()
        );

        // Everywhere else the flag changes nothing: there is nothing to accept.
        for plain in [SoStatus::Draft, SoStatus::Confirmed] {
            assert!(plain.ensure_cancellable(false).is_ok(), "{plain:?}");
            assert!(plain.ensure_cancellable(true).is_ok(), "{plain:?}");
        }
        // And it never resurrects a closed order.
        for closed in [SoStatus::Delivered, SoStatus::Cancelled] {
            assert!(closed.ensure_cancellable(true).is_err(), "{closed:?}");
        }
    }

    /// A header with a given status and promised date; nothing else about an
    /// order matters to the lateness predicate.
    fn promising(status: SoStatus, when: Option<Date>) -> SalesOrder {
        SalesOrder {
            id: InvSalesOrderId::new("so"),
            customer_id: BillingCustomerId::new("cus"),
            status,
            currency: "EUR".to_owned(),
            number: Some("SO-2026-00001".to_owned()),
            confirmed_date: when,
            expected_date: when,
            closed_date: None,
            reference: String::new(),
            note: String::new(),
            created_by: "u".to_owned(),
            created_at: OffsetDateTime::UNIX_EPOCH,
            updated_at: OffsetDateTime::UNIX_EPOCH,
        }
    }

    #[test]
    fn only_an_open_order_past_the_day_we_promised_is_late() {
        let today = Date::from_calendar_date(2026, time::Month::August, 10)
            .unwrap_or_else(|e| panic!("{e}"));
        let day_before = today
            .previous_day()
            .unwrap_or_else(|| panic!("no yesterday"));
        let day_after = today.next_day().unwrap_or_else(|| panic!("no tomorrow"));

        assert!(promising(SoStatus::Confirmed, Some(day_before)).is_late(today));
        assert!(promising(SoStatus::PartiallyDelivered, Some(day_before)).is_late(today));
        // Promised today means we still have the day.
        assert!(!promising(SoStatus::Confirmed, Some(today)).is_late(today));
        assert!(!promising(SoStatus::Confirmed, Some(day_after)).is_late(today));
        // Nobody said when: nothing to be late against.
        assert!(!promising(SoStatus::Confirmed, None).is_late(today));
        // Never confirmed, or finished with: nothing owed.
        for other in [SoStatus::Draft, SoStatus::Delivered, SoStatus::Cancelled] {
            assert!(
                !promising(other, Some(day_before)).is_late(today),
                "{other:?} owes nothing"
            );
        }
    }

    #[test]
    fn a_new_order_defaults_to_the_customers_currency() {
        let input = NewSalesOrder::for_customer(BillingCustomerId::new("cus"));
        assert!(
            input.currency.is_none(),
            "None means: take the customer's currency"
        );
        assert!(input.expected_date.is_none());
        assert!(input.reference.is_empty() && input.note.is_empty());
    }
}
