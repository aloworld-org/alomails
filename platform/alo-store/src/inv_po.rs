//! Purchase orders — what we asked a supplier for (alo Inventory, ADR 0035,
//! wave B5.05a), reached through the account door like every other business
//! record.
//!
//! A purchase order has the same two halves of a life a quote has
//! ([`crate::billing_quotes`]): a **draft** that is freely editable and carries
//! no number, and a document that has left the building and is frozen. What
//! differs is the counterparty — a supplier, not a customer — and how it ends:
//! an offer ends in an answer, an order ends in **goods arriving**.
//!
//! **The lifecycle is `draft → sent → partially_received → received`, with
//! `cancelled` reachable from the three states that are not terminal**, and
//! nothing else. Every transition goes through one pure table
//! ([`PoStatus::can_advance_to`]), so the rules are unit-tested over all
//! twenty-five ordered pairs rather than scattered across the write paths.
//! `received` and `cancelled` are terminal: an order that arrived, or that we
//! stopped expecting, is not re-opened by editing a status — the answer to "we
//! want more of it" is another order, which is also what keeps our copy and the
//! supplier's copy the same document.
//!
//! Two transitions are **not** written here, deliberately, and the reason is
//! the one this module cares most about:
//!
//! - **Sending** ([`PoStatus::Sent`]) draws the number, stamps the order date
//!   and writes the covering mail draft with the order attached — one act, in
//!   one transaction, because a purchase order's *sent* state means precisely
//!   "we have asked them" (`docs/design/inventory.md`). Splitting the mail off
//!   would let a tenant hold an order marked sent that nobody ever sent, which
//!   is the state that makes a shortage report lie. It arrives whole, with the
//!   paper, in B5.05a2.
//! - **Receiving** ([`PoStatus::PartiallyReceived`], [`PoStatus::Received`]) is
//!   entered by a receipt and never by a person choosing a status: it is the
//!   consequence of movements into stock (B5.05b).
//!
//! Until those exist, the states are in the vocabulary, in the database's CHECK
//! and in the transition table — so the code that writes them is held to rules
//! that were settled before it, rather than settling them itself.
//!
//! Money is never stored: net, VAT and gross are derived from the lines on
//! every read by [`crate::billing_totals`], so a total can never drift from the
//! lines that justify it. The lines themselves are [`crate::inv_po_lines`]'.
//!
//! Tenancy is structural: every statement carries `tenant_id` from the handle,
//! the supplier link and every product a line names are re-checked under the
//! same handle before they are written, and the database backs both with
//! composite foreign keys.

use time::{Date, OffsetDateTime};

use crate::account::AccountStore;
use crate::billing_field::{bounded, currency};
use crate::billing_invoices::{INVOICE_NOTE_MAX_CHARS, INVOICE_REFERENCE_MAX_CHARS};
use crate::billing_line::{FiguresRow, group_figures};
use crate::billing_totals::{LineFigures, Totals, totals};
use crate::error::{Result, StoreError};
use crate::id::{InvPurchaseOrderId, InvSupplierId};
use crate::inv_po_lines::{self, NewPoLine, NormalizedPoLine, PoLine};

/// The columns every read of an order selects, in `PoRow` order.
const PO_COLS: &str = "id, supplier_id, status, currency, number, ordered_date, expected_date, \
     closed_date, reference, note, created_by, created_at, updated_at";

/// Where an order is in its life.
///
/// `draft → sent → partially_received → received`, with `cancelled` reachable
/// from every state that is not already terminal.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PoStatus {
    /// Editable, unnumbered, not yet placed with anybody.
    Draft,
    /// Numbered, dated and frozen: we have asked them.
    Sent,
    /// Some quantity of some line has arrived. Entered by a receipt, never by
    /// a person.
    PartiallyReceived,
    /// Every line's received quantity equals what was ordered. Terminal.
    Received,
    /// We stopped expecting the goods. Terminal.
    Cancelled,
}

impl PoStatus {
    /// The value stored in the `status` column.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Draft => "draft",
            Self::Sent => "sent",
            Self::PartiallyReceived => "partially_received",
            Self::Received => "received",
            Self::Cancelled => "cancelled",
        }
    }

    /// Parses a stored status, or `None` if it is not one we know.
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "draft" => Some(Self::Draft),
            "sent" => Some(Self::Sent),
            "partially_received" => Some(Self::PartiallyReceived),
            "received" => Some(Self::Received),
            "cancelled" => Some(Self::Cancelled),
            _ => None,
        }
    }

    /// Whether the order is still editable — and therefore also deletable, and
    /// still without a number.
    pub fn is_draft(self) -> bool {
        matches!(self, Self::Draft)
    }

    /// Whether the order has been placed with the supplier, and so carries a
    /// number and an order date. `cancelled` is deliberately not here: an order
    /// cancelled while still a draft was never placed, and one cancelled after
    /// sending keeps the number it was sent under.
    pub fn is_placed(self) -> bool {
        matches!(self, Self::Sent | Self::PartiallyReceived | Self::Received)
    }

    /// Whether the order is finished with, one way or the other — and therefore
    /// stamped with the day it closed.
    pub fn is_closed(self) -> bool {
        matches!(self, Self::Received | Self::Cancelled)
    }

    /// Whether goods may still arrive against this order — what the shortage
    /// report (B5.07) means by "on order".
    pub fn is_open(self) -> bool {
        matches!(self, Self::Sent | Self::PartiallyReceived)
    }

    /// The states this one may move to. **The whole lifecycle is this
    /// function**: a draft is sent, a sent order is received in part or in
    /// full, a partly-received one completes, and anything not yet finished can
    /// be given up on.
    pub fn allowed_next(self) -> &'static [Self] {
        match self {
            Self::Draft => &[Self::Sent, Self::Cancelled],
            Self::Sent => &[Self::PartiallyReceived, Self::Received, Self::Cancelled],
            Self::PartiallyReceived => &[Self::Received, Self::Cancelled],
            Self::Received | Self::Cancelled => &[],
        }
    }

    /// Whether `to` is a legal move from this state. Never true for `to ==
    /// self`: re-sending an order that is already out would draw a second
    /// number, and cancelling twice is a caller that has lost track of the
    /// document, which answering "fine" would hide.
    ///
    /// A second **receipt** against a partly-received order is not a repeated
    /// transition — it is a new receipt that leaves the order in the same
    /// state, and B5.05b writes it without asking this table.
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
    /// in [`PoStatus::allowed_next`].
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
            "a purchase order cannot become {} while it is {}; {allowance}",
            to.as_str(),
            self.as_str()
        )))
    }

    /// The guard every write path runs before it changes an order's content: a
    /// draft may be edited and deleted, anything else is frozen.
    ///
    /// Changing a line after the supplier has the paper would make our copy
    /// disagree with theirs, and the correction for that is a new order.
    ///
    /// # Errors
    /// [`StoreError::Conflict`] naming the status that refused (`409`).
    pub fn ensure_editable(self) -> Result<()> {
        if self.is_draft() {
            return Ok(());
        }
        Err(StoreError::Conflict(format!(
            "a purchase order can only be changed while it is a draft; this one is {}",
            self.as_str()
        )))
    }

    /// The guard cancelling runs **in addition** to the transition table:
    /// giving up on an order some of whose goods have already arrived is
    /// accepting a short delivery as final, so it has to be said out loud.
    ///
    /// `short_close` is the caller's explicit "yes, that is all we are getting".
    /// It is ignored from every other state, where cancelling needs no such
    /// admission.
    ///
    /// # Errors
    /// [`StoreError::Conflict`] when the order is partly received and the
    /// caller has not accepted the shortfall.
    pub fn ensure_cancellable(self, short_close: bool) -> Result<()> {
        self.ensure_transition(Self::Cancelled)?;
        if matches!(self, Self::PartiallyReceived) && !short_close {
            return Err(StoreError::Conflict(
                "part of this order has already arrived; cancelling it accepts the short \
                 delivery as final, which has to be asked for explicitly"
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
fn parse_stored_status(stored: &str) -> Result<PoStatus> {
    PoStatus::parse(stored).ok_or_else(|| {
        StoreError::Db(sqlx::Error::Decode(
            "inv_purchase_orders.status is not a known status".into(),
        ))
    })
}

/// The writable header of an order, used for both create and update (an update
/// is a full replace — the route layer merges a partial `PATCH` onto the stored
/// record before calling). Lines are written separately, as a set.
///
/// `currency` is `None` to mean *take the supplier's*. Whatever is resolved is
/// **stored on the document**: a supplier who re-denominates their price list
/// next year must not restate an order placed this year.
#[derive(Debug, Clone)]
pub struct NewPurchaseOrder {
    /// Who we are buying from. Must be one of this tenant's suppliers.
    pub supplier_id: InvSupplierId,
    /// ISO 4217 code, or `None` for the supplier's default.
    pub currency: Option<String>,
    /// When we expect the goods, or `None` while nobody has said.
    pub expected_date: Option<Date>,
    /// Our own reference for the order — a project code, their quotation
    /// number — printed on the document.
    pub reference: String,
    /// Free-text note printed under the lines.
    pub note: String,
}

impl NewPurchaseOrder {
    /// The blank header a new draft starts from: this supplier, their currency,
    /// no expected date, no reference and no note. There is deliberately no
    /// [`Default`] — an order without a supplier is not a document.
    pub fn for_supplier(supplier_id: InvSupplierId) -> Self {
        Self {
            supplier_id,
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
pub struct PurchaseOrder {
    /// Opaque id, unique within the tenant.
    pub id: InvPurchaseOrderId,
    /// Who we are buying from.
    pub supplier_id: InvSupplierId,
    /// Where the order is in its life.
    pub status: PoStatus,
    /// ISO 4217 code the order was placed in.
    pub currency: String,
    /// The document number, `None` until the order is sent.
    pub number: Option<String>,
    /// The day we asked them, `None` until the order is sent.
    pub ordered_date: Option<Date>,
    /// When the goods are expected, `None` while nobody has said.
    pub expected_date: Option<Date>,
    /// The day the order was received in full or cancelled; `None` while it is
    /// still open (or was never placed).
    pub closed_date: Option<Date>,
    /// Our own reference.
    pub reference: String,
    /// Free-text note.
    pub note: String,
    /// The user who raised the order.
    pub created_by: String,
    /// Creation time.
    pub created_at: OffsetDateTime,
    /// Last modification time — moved by a header edit, a line edit and every
    /// transition, since all three change what the document says.
    pub updated_at: OffsetDateTime,
}

impl PurchaseOrder {
    /// Whether the goods are **late**: the order is still open and we expected
    /// them before `today`.
    ///
    /// Derived, never stored — a stored flag would be wrong every midnight, the
    /// same rule an invoice's overdue flag and a quote's lapsed flag follow. An
    /// order nobody has placed, and one that is finished with, are never late
    /// whatever the dates say.
    pub fn is_late(&self, today: Date) -> bool {
        self.status.is_open() && self.expected_date.is_some_and(|when| when < today)
    }
}

/// An order as a list entry: the header, who it is with, and what it is worth —
/// without the lines. The totals are computed, never read from a column.
#[derive(Debug, Clone)]
pub struct PurchaseOrderSummary {
    /// The header.
    pub order: PurchaseOrder,
    /// The supplier's name as it stands now. An id is not an explanation, and
    /// this list is read by a person.
    pub supplier_name: String,
    /// Net, VAT breakdown and gross, derived from the lines.
    pub totals: Totals,
}

/// A whole order: header, supplier name, lines in print order, and the totals
/// derived from those lines.
#[derive(Debug, Clone)]
pub struct PurchaseOrderDocument {
    /// The header.
    pub order: PurchaseOrder,
    /// The supplier's name as it stands now.
    pub supplier_name: String,
    /// The lines, in print order.
    pub lines: Vec<PoLine>,
    /// Net, VAT breakdown and gross, derived from `lines`.
    pub totals: Totals,
}

/// The header, validated and with the supplier's defaults resolved.
#[derive(Debug)]
struct NormalizedPo {
    supplier_id: String,
    currency: String,
    expected_date: Option<Date>,
    reference: String,
    note: String,
}

impl AccountStore {
    /// Resolves a header against **this tenant's** supplier: the supplier must
    /// exist under this handle, so a guessed id from another tenant is a
    /// `NotFound`, and it must be active — archiving a supplier means "we no
    /// longer buy from them", so ordering from them is a mistake worth
    /// reporting rather than obeying.
    ///
    /// The expected date is taken as given, including one in the past: an order
    /// typed up from a paper one placed last week is an ordinary thing to do,
    /// and refusing it would only teach people to lie about the date.
    async fn normalize_purchase_order(&self, input: &NewPurchaseOrder) -> Result<NormalizedPo> {
        let supplier = self
            .inv_supplier(&input.supplier_id)
            .await?
            .ok_or(StoreError::NotFound)?;
        if supplier.is_archived() {
            return Err(StoreError::Validation(
                "the supplier is archived; restore them before ordering from them again".to_owned(),
            ));
        }
        let resolved_currency = match input.currency.as_deref() {
            Some(code) => currency(code)?,
            None => supplier.currency,
        };
        Ok(NormalizedPo {
            supplier_id: supplier.id.as_str().to_owned(),
            currency: resolved_currency,
            expected_date: input.expected_date,
            reference: bounded("reference", &input.reference, INVOICE_REFERENCE_MAX_CHARS)?,
            note: bounded("note", &input.note, INVOICE_NOTE_MAX_CHARS)?,
        })
    }

    /// Validates a line set and holds every product it names to **this
    /// tenant's** catalog.
    ///
    /// A product that is not ours is a `NotFound` — existence is never
    /// disclosed — and an archived one is a `Validation` naming the line, since
    /// archiving a product means the tenant has stopped carrying it and buying
    /// more is a mistake worth reporting. Both are decided before a single row
    /// is written.
    async fn normalize_po_lines(&self, lines: &[NewPoLine]) -> Result<Vec<NormalizedPoLine>> {
        let normalized = inv_po_lines::normalize_po_lines(lines)?;
        let named = inv_po_lines::products_named(&normalized);
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
                     ordering more of it",
                    index + 1
                )));
            }
        }
        Ok(normalized)
    }

    /// Takes the order's row lock inside `tx` and returns its status, so a
    /// caller can check whether it may write and then write without any other
    /// transaction slipping in between. Two writers to one order serialise
    /// here; a writer that arrives after a send sees `sent`.
    ///
    /// # Errors
    /// [`StoreError::NotFound`] when the id is absent **or another tenant's**;
    /// [`StoreError::Db`] on failure or on a status the code does not know.
    async fn lock_purchase_order(
        &self,
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
        id: &InvPurchaseOrderId,
    ) -> Result<PoStatus> {
        let stored: Option<String> = sqlx::query_scalar(
            "SELECT status FROM inv_purchase_orders WHERE tenant_id = $1 AND id = $2 FOR UPDATE",
        )
        .bind(self.tenant.as_str())
        .bind(id.as_str())
        .fetch_optional(&mut **tx)
        .await
        .map_err(StoreError::Db)?;
        parse_stored_status(&stored.ok_or(StoreError::NotFound)?)
    }

    /// The status of one of this tenant's orders, without taking a lock — the
    /// cheap pre-check that lets a write refuse a frozen document before it
    /// does any other work. It is never the authority: every write re-reads the
    /// status under [`AccountStore::lock_purchase_order`] before writing.
    ///
    /// # Errors
    /// [`StoreError::NotFound`] when the id is absent or another tenant's;
    /// [`StoreError::Db`] on failure.
    async fn purchase_order_status(&self, id: &InvPurchaseOrderId) -> Result<PoStatus> {
        let stored: Option<String> = sqlx::query_scalar(
            "SELECT status FROM inv_purchase_orders WHERE tenant_id = $1 AND id = $2",
        )
        .bind(self.tenant.as_str())
        .bind(id.as_str())
        .fetch_optional(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        parse_stored_status(&stored.ok_or(StoreError::NotFound)?)
    }

    /// Creates a **draft** order with no lines — the state a new order starts
    /// in. It carries no number and no order date by construction; only sending
    /// assigns those.
    ///
    /// # Errors
    /// [`StoreError::NotFound`] when the supplier is not this tenant's;
    /// [`StoreError::Validation`] when the supplier is archived or a header
    /// field breaks its rule; [`StoreError::Db`] on failure.
    pub async fn create_inv_purchase_order(
        &self,
        input: &NewPurchaseOrder,
    ) -> Result<InvPurchaseOrderId> {
        let header = self.normalize_purchase_order(input).await?;
        let id = InvPurchaseOrderId::generate();
        sqlx::query(
            "INSERT INTO inv_purchase_orders (tenant_id, id, supplier_id, status, currency, \
                 expected_date, reference, note, created_by) \
             VALUES ($1, $2, $3, 'draft', $4, $5, $6, $7, $8)",
        )
        .bind(self.tenant.as_str())
        .bind(id.as_str())
        .bind(&header.supplier_id)
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

    /// The tenant's orders, newest first, each with its supplier's name and its
    /// computed totals. `status` filters; `None` lists everything.
    ///
    /// The lines of every listed order are fetched in one further statement,
    /// not one per order.
    ///
    /// # Errors
    /// [`StoreError::Db`] on failure.
    pub async fn inv_purchase_orders(
        &self,
        status: Option<PoStatus>,
    ) -> Result<Vec<PurchaseOrderSummary>> {
        let status = status.map(PoStatus::as_str);
        let rows = sqlx::query_as::<_, PoRow>(&format!(
            "SELECT {PO_COLS}, \
                 (SELECT name FROM inv_suppliers s \
                   WHERE s.tenant_id = o.tenant_id AND s.id = o.supplier_id) AS supplier_name \
             FROM inv_purchase_orders o \
             WHERE tenant_id = $1 AND ($2::text IS NULL OR status = $2) \
             ORDER BY created_at DESC, id"
        ))
        .bind(self.tenant.as_str())
        .bind(status)
        .fetch_all(&self.pool)
        .await
        .map_err(StoreError::Db)?;

        let figures = sqlx::query_as::<_, FiguresRow>(
            "SELECT po_id AS doc_id, qty_milli, unit_price_cents, vat_rate_bp \
             FROM inv_purchase_order_lines \
             WHERE tenant_id = $1 AND po_id IN ( \
                 SELECT id FROM inv_purchase_orders \
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
                let supplier_name = row.supplier_name.clone().unwrap_or_default();
                Ok(PurchaseOrderSummary {
                    order: row.into_order()?,
                    supplier_name,
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
    pub async fn inv_purchase_order(
        &self,
        id: &InvPurchaseOrderId,
    ) -> Result<Option<PurchaseOrderDocument>> {
        let Some(row) = sqlx::query_as::<_, PoRow>(&format!(
            "SELECT {PO_COLS}, \
                 (SELECT name FROM inv_suppliers s \
                   WHERE s.tenant_id = o.tenant_id AND s.id = o.supplier_id) AS supplier_name \
             FROM inv_purchase_orders o WHERE tenant_id = $1 AND id = $2"
        ))
        .bind(self.tenant.as_str())
        .bind(id.as_str())
        .fetch_optional(&self.pool)
        .await
        .map_err(StoreError::Db)?
        else {
            return Ok(None);
        };
        let supplier_name = row.supplier_name.clone().unwrap_or_default();
        let lines = inv_po_lines::read(&self.pool, self.tenant.as_str(), id.as_str()).await?;
        let figures: Vec<LineFigures> = lines.iter().map(PoLine::figures).collect();
        Ok(Some(PurchaseOrderDocument {
            order: row.into_order()?,
            supplier_name,
            lines,
            totals: totals(&figures),
        }))
    }

    /// The id of the tenant's order **numbered** `number`, or `None`.
    ///
    /// The counterpart of [`AccountStore::billing_invoice_id_by_number`], and
    /// for the same reason: a person — and, from B5.10, the inventory agent —
    /// says "PO-2026-00001", never an opaque id. Case-insensitive, blanks
    /// trimmed, otherwise exact; a draft has no number and is therefore
    /// unreachable here; another tenant's number is `None`.
    ///
    /// # Errors
    /// [`StoreError::Db`] on failure.
    pub async fn inv_purchase_order_id_by_number(
        &self,
        number: &str,
    ) -> Result<Option<InvPurchaseOrderId>> {
        let wanted = number.trim();
        if wanted.is_empty() {
            return Ok(None);
        }
        let id: Option<String> = sqlx::query_scalar(
            "SELECT id FROM inv_purchase_orders \
             WHERE tenant_id = $1 AND upper(number) = upper($2)",
        )
        .bind(self.tenant.as_str())
        .bind(wanted)
        .fetch_optional(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        Ok(id.map(InvPurchaseOrderId::new))
    }

    /// Replaces the writable header of a **draft** order: supplier, currency,
    /// expected date, reference and note. Status, number and the two stamped
    /// dates are not writable here — they move only through the lifecycle
    /// actions.
    ///
    /// The status is checked before the header is even validated, so a frozen
    /// document is told it is frozen rather than being handed a complaint about
    /// a field it was never going to accept; it is then re-checked under the row
    /// lock that the write itself takes.
    ///
    /// # Errors
    /// [`StoreError::NotFound`] when the order or the supplier is not this
    /// tenant's; [`StoreError::Conflict`] when the order is no longer a draft;
    /// [`StoreError::Validation`] as for create; [`StoreError::Db`] on failure.
    pub async fn update_inv_purchase_order(
        &self,
        id: &InvPurchaseOrderId,
        input: &NewPurchaseOrder,
    ) -> Result<()> {
        self.purchase_order_status(id).await?.ensure_editable()?;
        let header = self.normalize_purchase_order(input).await?;

        let mut tx = self.pool.begin().await.map_err(StoreError::Db)?;
        // Authoritative: the state that matters is the one under the lock the
        // UPDATE below writes through, not the one read a moment ago.
        // Dropping the transaction on any error rolls it back untouched.
        self.lock_purchase_order(&mut tx, id)
            .await?
            .ensure_editable()?;
        sqlx::query(
            "UPDATE inv_purchase_orders SET supplier_id = $3, currency = $4, expected_date = $5, \
                 reference = $6, note = $7, updated_at = now() \
             WHERE tenant_id = $1 AND id = $2",
        )
        .bind(self.tenant.as_str())
        .bind(id.as_str())
        .bind(&header.supplier_id)
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
    /// Every line is validated, and every product it names held to this
    /// tenant's catalog, before anything is written; the draft-only guard runs
    /// before even that, under the same lock the replacement writes through — so
    /// a set cannot land on an order that was sent while it was being composed.
    ///
    /// # Errors
    /// [`StoreError::NotFound`] when the order, or a product a line names, is
    /// not this tenant's; [`StoreError::Conflict`] when the order is no longer a
    /// draft; [`StoreError::Validation`] when the set is too long or a line
    /// breaks a field rule (the message names the line's position);
    /// [`StoreError::Db`] on failure.
    pub async fn set_inv_purchase_order_lines(
        &self,
        id: &InvPurchaseOrderId,
        lines: &[NewPoLine],
    ) -> Result<()> {
        self.purchase_order_status(id).await?.ensure_editable()?;
        let lines = self.normalize_po_lines(lines).await?;

        let mut tx = self.pool.begin().await.map_err(StoreError::Db)?;
        self.lock_purchase_order(&mut tx, id)
            .await?
            .ensure_editable()?;
        inv_po_lines::replace(&mut tx, self.tenant.as_str(), id.as_str(), &lines).await?;
        sqlx::query(
            "UPDATE inv_purchase_orders SET updated_at = now() WHERE tenant_id = $1 AND id = $2",
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
    /// placed with anybody. One that has been sent is cancelled instead, keeping
    /// its number and its content readable.
    ///
    /// # Errors
    /// [`StoreError::NotFound`] when the order is absent or another tenant's;
    /// [`StoreError::Conflict`] when it is no longer a draft;
    /// [`StoreError::Db`] on failure.
    pub async fn delete_inv_purchase_order(&self, id: &InvPurchaseOrderId) -> Result<()> {
        let mut tx = self.pool.begin().await.map_err(StoreError::Db)?;
        self.lock_purchase_order(&mut tx, id)
            .await?
            .ensure_editable()?;
        sqlx::query("DELETE FROM inv_purchase_orders WHERE tenant_id = $1 AND id = $2")
            .bind(self.tenant.as_str())
            .bind(id.as_str())
            .execute(&mut *tx)
            .await
            .map_err(StoreError::Db)?;
        tx.commit().await.map_err(StoreError::Db)?;
        Ok(())
    }

    /// Cancels an order: we have stopped expecting the goods.
    ///
    /// Legal from `draft` (an order nobody placed, kept rather than deleted so
    /// the decision is on the record), from `sent`, and from
    /// `partially_received` only when the caller passes `short_close` — that is
    /// a decision to accept a short delivery as final, and it must be asked for
    /// rather than slipped into.
    ///
    /// The closing date is the database's `CURRENT_DATE`, read inside the same
    /// transaction as the write and never supplied by the caller. A cancelled
    /// order keeps whatever number it had: one cancelled as a draft never had
    /// one, and one cancelled after sending keeps the number the supplier holds.
    ///
    /// This store call sends no email. Telling the supplier is a letter the
    /// tenant writes, and it goes through the one audited submission path like
    /// every other message this product composes.
    ///
    /// # Errors
    /// [`StoreError::NotFound`] when the order is absent or another tenant's;
    /// [`StoreError::Conflict`] when it is already closed, or partly received
    /// without `short_close`; [`StoreError::Db`] on failure.
    pub async fn cancel_inv_purchase_order(
        &self,
        id: &InvPurchaseOrderId,
        short_close: bool,
    ) -> Result<PurchaseOrderDocument> {
        let mut tx = self.pool.begin().await.map_err(StoreError::Db)?;
        self.lock_purchase_order(&mut tx, id)
            .await?
            .ensure_cancellable(short_close)?;
        sqlx::query(
            "UPDATE inv_purchase_orders SET status = 'cancelled', closed_date = CURRENT_DATE, \
                 updated_at = now() \
             WHERE tenant_id = $1 AND id = $2",
        )
        .bind(self.tenant.as_str())
        .bind(id.as_str())
        .execute(&mut *tx)
        .await
        .map_err(StoreError::Db)?;
        tx.commit().await.map_err(StoreError::Db)?;

        self.inv_purchase_order(id)
            .await?
            .ok_or(StoreError::NotFound)
    }
}

// ---- row types --------------------------------------------------------------

#[derive(sqlx::FromRow)]
struct PoRow {
    id: String,
    supplier_id: String,
    status: String,
    currency: String,
    number: Option<String>,
    ordered_date: Option<Date>,
    expected_date: Option<Date>,
    closed_date: Option<Date>,
    reference: String,
    note: String,
    created_by: String,
    created_at: OffsetDateTime,
    updated_at: OffsetDateTime,
    /// Joined, not stored: the supplier's current name. `None` cannot happen
    /// while the foreign key holds — it is treated as an empty name rather than
    /// a failure, because a list must not disappear over a display string.
    supplier_name: Option<String>,
}

impl PoRow {
    fn into_order(self) -> Result<PurchaseOrder> {
        let status = parse_stored_status(&self.status)?;
        Ok(PurchaseOrder {
            id: InvPurchaseOrderId::new(self.id),
            supplier_id: InvSupplierId::new(self.supplier_id),
            status,
            currency: self.currency,
            number: self.number,
            ordered_date: self.ordered_date,
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

    /// Every state, in one place, so a state added later fails these tests
    /// until its rules are stated.
    const ALL: [PoStatus; 5] = [
        PoStatus::Draft,
        PoStatus::Sent,
        PoStatus::PartiallyReceived,
        PoStatus::Received,
        PoStatus::Cancelled,
    ];

    #[test]
    fn every_status_round_trips_through_its_stored_form() {
        for status in ALL {
            assert_eq!(PoStatus::parse(status.as_str()), Some(status));
        }
        assert!(PoStatus::Draft.is_draft());
        for other in [
            PoStatus::Sent,
            PoStatus::PartiallyReceived,
            PoStatus::Received,
            PoStatus::Cancelled,
        ] {
            assert!(!other.is_draft());
        }
    }

    #[test]
    fn an_unknown_stored_status_is_never_guessed_at() {
        // Including a quote's and an invoice's states: the documents do not
        // share a status vocabulary, and reading one as another would be a
        // frozen document silently becoming editable.
        for bad in [
            "",
            "Draft",
            "DRAFT",
            "issued",
            "accepted",
            "partially received",
            "sent ",
        ] {
            assert_eq!(PoStatus::parse(bad), None, "{bad:?}");
        }
        match parse_stored_status("accepted") {
            Err(StoreError::Db(_)) => {}
            other => panic!("expected a decode failure, got {other:?}"),
        }
        assert!(parse_stored_status("partially_received").is_ok());
    }

    /// The whole lifecycle, stated once as data: exactly these ordered pairs
    /// are legal and the other eighteen are not.
    const LEGAL: [(PoStatus, PoStatus); 7] = [
        (PoStatus::Draft, PoStatus::Sent),
        (PoStatus::Draft, PoStatus::Cancelled),
        (PoStatus::Sent, PoStatus::PartiallyReceived),
        (PoStatus::Sent, PoStatus::Received),
        (PoStatus::Sent, PoStatus::Cancelled),
        (PoStatus::PartiallyReceived, PoStatus::Received),
        (PoStatus::PartiallyReceived, PoStatus::Cancelled),
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
        // Re-sending would draw a second number; cancelling twice is a caller
        // that has lost track of the document. A second receipt against a
        // partly-received order is not this — it is a receipt, not a
        // transition (B5.05b).
        for status in ALL {
            assert!(!status.can_advance_to(status), "{status:?}");
        }
    }

    #[test]
    fn a_finished_order_never_reopens() {
        for closed in [PoStatus::Received, PoStatus::Cancelled] {
            assert!(closed.is_closed());
            assert!(
                closed.allowed_next().is_empty(),
                "{closed:?} is terminal: the answer to wanting more is another order"
            );
            let message = match closed.ensure_transition(PoStatus::Sent) {
                Err(StoreError::Conflict(message)) => message,
                other => panic!("expected Conflict for {closed:?}, got {other:?}"),
            };
            assert!(message.contains(closed.as_str()), "{message}");
            assert!(message.contains("closed"), "{message}");
        }
        for unfinished in [PoStatus::Draft, PoStatus::Sent, PoStatus::PartiallyReceived] {
            assert!(!unfinished.is_closed());
        }
    }

    #[test]
    fn only_a_placed_order_carries_a_number_and_only_an_open_one_expects_goods() {
        // The two predicates the database's CHECKs mirror, and the one the
        // shortage report will ask.
        for placed in [
            PoStatus::Sent,
            PoStatus::PartiallyReceived,
            PoStatus::Received,
        ] {
            assert!(placed.is_placed(), "{placed:?}");
        }
        for unplaced in [PoStatus::Draft, PoStatus::Cancelled] {
            assert!(!unplaced.is_placed(), "{unplaced:?}");
        }
        for open in [PoStatus::Sent, PoStatus::PartiallyReceived] {
            assert!(open.is_open(), "{open:?}");
        }
        for shut in [PoStatus::Draft, PoStatus::Received, PoStatus::Cancelled] {
            assert!(!shut.is_open(), "{shut:?}");
        }
    }

    #[test]
    fn a_refused_transition_says_what_is_allowed_instead() {
        // The UI corrects itself from the refusal rather than by a second round
        // trip — and the message carries rules, never supplier data.
        let message = match PoStatus::Draft.ensure_transition(PoStatus::Received) {
            Err(StoreError::Conflict(message)) => message,
            other => panic!("expected Conflict, got {other:?}"),
        };
        assert!(
            message.contains("received") && message.contains("draft"),
            "{message}"
        );
        assert!(message.contains("sent"), "a draft can be sent: {message}");
        assert!(message.contains("cancelled"), "or given up on: {message}");
    }

    #[test]
    fn only_a_draft_may_be_changed() {
        assert!(PoStatus::Draft.ensure_editable().is_ok());
        for frozen in [
            PoStatus::Sent,
            PoStatus::PartiallyReceived,
            PoStatus::Received,
            PoStatus::Cancelled,
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
    fn giving_up_on_a_part_delivered_order_has_to_be_said_out_loud() {
        // Cancelling what has already partly arrived accepts the shortfall as
        // final — a decision, not a slip.
        let message = match PoStatus::PartiallyReceived.ensure_cancellable(false) {
            Err(StoreError::Conflict(message)) => message,
            other => panic!("expected Conflict, got {other:?}"),
        };
        assert!(message.contains("short delivery"), "{message}");
        assert!(PoStatus::PartiallyReceived.ensure_cancellable(true).is_ok());

        // Everywhere else the flag changes nothing: there is nothing to accept.
        for plain in [PoStatus::Draft, PoStatus::Sent] {
            assert!(plain.ensure_cancellable(false).is_ok(), "{plain:?}");
            assert!(plain.ensure_cancellable(true).is_ok(), "{plain:?}");
        }
        // And it never resurrects a closed order.
        for closed in [PoStatus::Received, PoStatus::Cancelled] {
            assert!(closed.ensure_cancellable(true).is_err(), "{closed:?}");
        }
    }

    /// A header with a given status and expected date; nothing else about an
    /// order matters to the lateness predicate.
    fn expecting(status: PoStatus, when: Option<Date>) -> PurchaseOrder {
        PurchaseOrder {
            id: InvPurchaseOrderId::new("po"),
            supplier_id: InvSupplierId::new("sup"),
            status,
            currency: "EUR".to_owned(),
            number: Some("PO-2026-00001".to_owned()),
            ordered_date: when,
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
    fn only_an_open_order_past_its_expected_day_is_late() {
        let today = Date::from_calendar_date(2026, time::Month::August, 10)
            .unwrap_or_else(|e| panic!("{e}"));
        let day_before = today
            .previous_day()
            .unwrap_or_else(|| panic!("no yesterday"));
        let day_after = today.next_day().unwrap_or_else(|| panic!("no tomorrow"));

        assert!(expecting(PoStatus::Sent, Some(day_before)).is_late(today));
        assert!(expecting(PoStatus::PartiallyReceived, Some(day_before)).is_late(today));
        // Expected today means they still have the day.
        assert!(!expecting(PoStatus::Sent, Some(today)).is_late(today));
        assert!(!expecting(PoStatus::Sent, Some(day_after)).is_late(today));
        // Nobody said when: nothing to be late against.
        assert!(!expecting(PoStatus::Sent, None).is_late(today));
        // Never placed, or finished with: not waiting on anything.
        for other in [PoStatus::Draft, PoStatus::Received, PoStatus::Cancelled] {
            assert!(
                !expecting(other, Some(day_before)).is_late(today),
                "{other:?} is not waiting on goods"
            );
        }
    }

    #[test]
    fn a_new_order_defaults_to_the_suppliers_currency() {
        let input = NewPurchaseOrder::for_supplier(InvSupplierId::new("sup"));
        assert!(
            input.currency.is_none(),
            "None means: take the supplier's currency"
        );
        assert!(input.expected_date.is_none());
        assert!(input.reference.is_empty() && input.note.is_empty());
    }
}
