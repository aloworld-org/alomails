//! Bills — supplier invoices we have received (alo Billing, ADR 0035, wave
//! B1.24), reached through the account door like every other billing record.
//!
//! A bill is the mirror of an invoice ([`crate::billing_invoices`]), and the
//! two differences are the whole design:
//!
//! - **We do not author it.** It carries the supplier's number, the supplier's
//!   dates and the supplier's totals. Nothing here draws from our gapless
//!   series, nothing here issues, voids or credits. The only thing a tenant
//!   decides about a bill is whether they accept it.
//! - **What is stored is what the document says.** The totals are copied from
//!   the file, not recomputed: the supplier's paper is the authority on what
//!   they are charging, and a stored figure that disagreed with it would be our
//!   arithmetic quietly overruling their invoice. The import nevertheless
//!   *reconciles* the document against itself before writing it
//!   ([`crate::billing_einvoice_import`]), so an incoherent invoice is refused
//!   at the door rather than booked and discovered at the year end.
//!
//! **One bill is drafted rather than received**, and it is the exception that
//! proves both rules: receiving a purchase order ([`crate::inv_po_receive`])
//! raises a bill for what we ordered and *actually took delivery of*, so that a
//! delivery is never silently unbilled while the supplier's own invoice is in
//! the post. It carries no syntax and no checksum, because it was read from no
//! file, and it is `received` like every other — nobody has decided about it,
//! and the supplier's real document arrives later as a bill of its own.
//!
//! **A decision is final.** `received → approved` or `received → rejected`, and
//! nothing else: an approved bill is a liability the accounts will carry, and
//! un-approving it after the fact would rewrite history. A bill approved by
//! mistake is corrected the way the paper world corrects one — the supplier
//! issues a credit note, which arrives as a bill of its own.
//!
//! **Duplicates are refused, not tolerated.** A supplier's number is unique
//! within that supplier by law, so `(supplier, number)` is the document's
//! identity: the same invoice forwarded twice and imported by two people is one
//! bill, and paying it twice is the specific accident this constraint exists to
//! prevent.
//!
//! Tenancy is structural: every statement carries `tenant_id` from the handle,
//! and the lines reach their tenant only through their bill, which the database
//! backs with a composite foreign key.

use sha2::{Digest, Sha256};
use time::{Date, OffsetDateTime};

use crate::account::AccountStore;
use crate::billing_einvoice_import::{EInvoiceSyntax, InboundInvoice, parse_einvoice};
use crate::billing_field::{bounded, country, currency, required};
use crate::billing_line::{BILL_LINES, Line, NewLine, normalize_lines};
use crate::billing_totals::{Totals, totals};
use crate::error::{Result, StoreError};
use crate::id::BillingBillId;

/// A supplier's name is bounded like a customer's: it is the same kind of
/// company name, and the two sit in the same lists.
pub const SUPPLIER_NAME_MAX_CHARS: usize = 200;
/// A document number is short: the longest series any European tenant prints
/// still fits, and a 200-character "number" is a paste accident.
pub const BILL_NUMBER_MAX_CHARS: usize = 60;
/// Free-text fields a bill carries from the document it was read from.
pub const BILL_TEXT_MAX_CHARS: usize = 500;
/// The largest total a bill may carry, in integer cents: €10 000 000 000.00.
/// A typo guard with an arithmetic job, chosen like every other ceiling in
/// billing — four orders of magnitude below `i64::MAX`.
pub const BILL_MAX_CENTS: i64 = 1_000_000_000_000;

/// The columns every read of a bill selects, in [`BillRow`] order.
///
/// `pub(crate)` for one sibling: the payment run ([`crate::billing_sepa`]) reads
/// bills through its own `WHERE`, and a second column list would be a second
/// place for a column to be forgotten.
pub(crate) const BILL_COLS: &str = "id, source_syntax, source_sha256, type_code, status, supplier_name, \
     supplier_vat_id, supplier_legal_id, supplier_line1, supplier_line2, supplier_postal_code, \
     supplier_city, supplier_country, supplier_email, supplier_iban, number, issue_date, \
     due_date, currency, buyer_reference, note, payment_reference, line_total_cents, \
     allowance_total_cents, charge_total_cents, tax_exclusive_cents, tax_total_cents, \
     tax_inclusive_cents, prepaid_cents, payable_cents, imported_by, imported_at, decided_by, \
     decided_at, exported_at, exported_by, export_message_id";

/// Where a received bill stands.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BillStatus {
    /// It has arrived and nobody has decided about it.
    Received,
    /// We accept it: the liability is real and it will be paid.
    Approved,
    /// We do not accept it. The document stays, because refusing to pay an
    /// invoice is a fact worth keeping, not a reason to delete it.
    Rejected,
}

impl BillStatus {
    /// The value this status is stored and reported as.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Received => "received",
            Self::Approved => "approved",
            Self::Rejected => "rejected",
        }
    }

    /// The status a stored value names, or `None` when it is not one of ours.
    #[must_use]
    pub fn parse(raw: &str) -> Option<Self> {
        match raw {
            "received" => Some(Self::Received),
            "approved" => Some(Self::Approved),
            "rejected" => Some(Self::Rejected),
            _ => None,
        }
    }

    /// Whether a decision has been made about the bill.
    #[must_use]
    pub fn is_decided(self) -> bool {
        !matches!(self, Self::Received)
    }
}

/// The supplier, as the document named them. Copied, never linked: a supplier
/// master record is B5.03, and a bill must stay readable whatever later happens
/// to it.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Supplier {
    /// Registered or trading name (BT-27).
    pub name: String,
    /// VAT identifier (BT-31), blank when none was stated.
    pub vat_id: String,
    /// Legal registration identifier (BT-30).
    pub legal_id: String,
    /// Address line 1 (BT-35).
    pub line1: String,
    /// Address line 2 (BT-36).
    pub line2: String,
    /// Post code (BT-38).
    pub postal_code: String,
    /// City (BT-37).
    pub city: String,
    /// ISO 3166-1 alpha-2 country (BT-40).
    pub country: String,
    /// Electronic address (BT-34).
    pub email: String,
    /// The account they ask to be paid into (BT-84).
    pub iban: String,
}

impl Supplier {
    /// The one comparable key for "who is this document from": the VAT
    /// identifier when they state one, otherwise the name folded to lower case.
    ///
    /// It exists for the duplicate constraint and nothing else. A VAT id is the
    /// right key because it survives a supplier renaming themselves; the name
    /// is the fallback because a small supplier may have no VAT id at all.
    #[must_use]
    pub fn key(&self) -> String {
        let vat_id = self.vat_id.trim();
        if vat_id.is_empty() {
            self.name.trim().to_lowercase()
        } else {
            vat_id.to_uppercase()
        }
    }
}

/// The totals the document states, in ledger direction (negative for a credit
/// note), in integer cents.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct BillTotals {
    /// Sum of the line amounts (BT-106).
    pub line_total_cents: i64,
    /// Document-level allowances (BT-107).
    pub allowance_total_cents: i64,
    /// Document-level charges (BT-108).
    pub charge_total_cents: i64,
    /// Total without VAT (BT-109).
    pub tax_exclusive_cents: i64,
    /// Total VAT (BT-110).
    pub tax_total_cents: i64,
    /// Total with VAT (BT-112).
    pub tax_inclusive_cents: i64,
    /// Already paid (BT-113).
    pub prepaid_cents: i64,
    /// Amount due for payment (BT-115).
    pub payable_cents: i64,
}

/// A bill as the caller states it — what the parser produces from a file, and
/// the shape a hand-entry surface would fill in.
#[derive(Debug, Clone, Default)]
pub struct NewBill {
    /// The syntax the document arrived in.
    pub source_syntax: Option<EInvoiceSyntax>,
    /// SHA-256 of the imported bytes, lower-case hex.
    pub source_sha256: String,
    /// Whether it is a credit note rather than an invoice.
    pub credit_note: bool,
    /// The supplier.
    pub supplier: Supplier,
    /// Their document number (BT-1).
    pub number: String,
    /// Their issue date (BT-2).
    pub issue_date: Option<Date>,
    /// Their due date (BT-9), when stated.
    pub due_date: Option<Date>,
    /// Document currency (BT-5).
    pub currency: String,
    /// The reference they quote for us (BT-10).
    pub buyer_reference: String,
    /// Document note (BT-22).
    pub note: String,
    /// The remittance reference to quote when paying (BT-83).
    pub payment_reference: String,
    /// The totals they state.
    pub totals: BillTotals,
    /// The lines, in document order.
    pub lines: Vec<NewLine>,
}

/// A stored bill.
#[derive(Debug, Clone)]
pub struct Bill {
    /// Opaque id, unique within the tenant.
    pub id: BillingBillId,
    /// The syntax it arrived in, or `None` for a bill that was not imported
    /// from a file.
    pub source_syntax: Option<EInvoiceSyntax>,
    /// SHA-256 of the imported bytes, lower-case hex.
    pub source_sha256: String,
    /// Whether it is a credit note (UNTDID 381) rather than an invoice (380).
    pub credit_note: bool,
    /// Where the approval stands.
    pub status: BillStatus,
    /// The supplier, as the document named them.
    pub supplier: Supplier,
    /// Their document number.
    pub number: String,
    /// Their issue date.
    pub issue_date: Date,
    /// Their due date, when stated.
    pub due_date: Option<Date>,
    /// Document currency.
    pub currency: String,
    /// The reference they quote for us.
    pub buyer_reference: String,
    /// Document note.
    pub note: String,
    /// The remittance reference to quote when paying.
    pub payment_reference: String,
    /// The totals they state, in ledger direction.
    pub totals: BillTotals,
    /// Who imported it.
    pub imported_by: String,
    /// When it was imported.
    pub imported_at: OffsetDateTime,
    /// Who approved or rejected it, when somebody has.
    pub decided_by: Option<String>,
    /// When that decision was made.
    pub decided_at: Option<OffsetDateTime>,
    /// When this bill was put into a SEPA payment file
    /// ([`crate::billing_sepa`]), if it has been. **Not** a payment: a file is
    /// an instruction to a bank, and the money moves when the bank says it did.
    pub exported_at: Option<OffsetDateTime>,
    /// Who instructed that payment.
    pub exported_by: Option<String>,
    /// The `MsgId` of the run it went into, which is what a bank quotes back.
    pub export_message_id: Option<String>,
}

/// A bill with its lines and the totals **we** compute from them.
///
/// Both sets of figures are reported: [`Bill::totals`] is what the supplier
/// says the document is worth, `computed` is what its lines add up to under our
/// own arithmetic. They agree for every document that can be imported — the
/// import refuses one where they do not — and showing both is what makes that
/// checkable by a person rather than only by a test.
#[derive(Debug, Clone)]
pub struct BillDocument {
    /// The stored bill.
    pub bill: Bill,
    /// Its lines, in document order.
    pub lines: Vec<Line>,
    /// What those lines add up to under our arithmetic.
    pub computed: Totals,
}

impl AccountStore {
    /// Imports an e-invoice file as a bill of this tenant.
    ///
    /// The single door for reading somebody else's invoice: the HTTP upload
    /// route goes through it, and so will the mail path that books an
    /// attachment. Parsing, the standard's own consistency checks, the sign
    /// convention and the write are one call, so no caller can perform half of
    /// them.
    ///
    /// # Errors
    /// [`StoreError::Validation`] when the file is not a readable e-invoice or
    /// states something we cannot hold exactly (the message names the business
    /// term or the rule, never the value); [`StoreError::Conflict`] when this
    /// supplier's document with this number has already been imported;
    /// [`StoreError::Db`] on failure.
    pub async fn import_billing_bill(&self, file: &[u8]) -> Result<BillingBillId> {
        let invoice = parse_einvoice(file)?.in_ledger_direction();
        let bill = new_bill_from(&invoice, &sha256_hex(file));
        self.create_billing_bill(&bill).await
    }

    /// Stores a bill.
    ///
    /// Separate from the import so that a hand-entered bill — a supplier who
    /// still sends paper — lands through exactly the same validation and the
    /// same duplicate rule.
    ///
    /// # Errors
    /// [`StoreError::Validation`] when a field breaks its rule;
    /// [`StoreError::Conflict`] on a duplicate; [`StoreError::Db`] on failure.
    pub async fn create_billing_bill(&self, input: &NewBill) -> Result<BillingBillId> {
        let mut tx = self.pool.begin().await.map_err(StoreError::Db)?;
        let id = self.create_billing_bill_in(&mut tx, input).await?;
        tx.commit().await.map_err(StoreError::Db)?;
        Ok(id)
    }

    /// [`AccountStore::create_billing_bill`], inside a transaction the caller
    /// owns.
    ///
    /// The bill and whatever raised it belong in **one** transaction: receiving
    /// a purchase order writes movements, the order's new state and this draft
    /// bill together ([`crate::inv_po_receive`]), and a tenant must never be
    /// left holding two of the three. Every rule and every refusal is the
    /// public door's; only the `BEGIN` and the `COMMIT` move to the caller.
    ///
    /// # Errors
    /// Exactly [`AccountStore::create_billing_bill`]'s. A caller must **not**
    /// catch them and carry on inside the same transaction: an error here has
    /// already poisoned it, and the only correct next step is to drop it.
    pub(crate) async fn create_billing_bill_in(
        &self,
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
        input: &NewBill,
    ) -> Result<BillingBillId> {
        let bill = normalize_bill(input)?;
        let lines = normalize_lines(&input.lines)?;
        if lines.is_empty() {
            return Err(StoreError::Validation(
                "a bill must have at least one line".to_owned(),
            ));
        }

        let id = BillingBillId::generate();
        let inserted = sqlx::query(
            "INSERT INTO billing_bills (tenant_id, id, source_syntax, source_sha256, type_code, \
                 supplier_name, supplier_vat_id, supplier_legal_id, supplier_line1, \
                 supplier_line2, supplier_postal_code, supplier_city, supplier_country, \
                 supplier_email, supplier_iban, supplier_key, number, issue_date, due_date, \
                 currency, buyer_reference, note, payment_reference, line_total_cents, \
                 allowance_total_cents, charge_total_cents, tax_exclusive_cents, tax_total_cents, \
                 tax_inclusive_cents, prepaid_cents, payable_cents, imported_by) \
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, $17, \
                 $18, $19, $20, $21, $22, $23, $24, $25, $26, $27, $28, $29, $30, $31, $32) \
             ON CONFLICT (tenant_id, supplier_key, number) DO NOTHING",
        )
        .bind(self.tenant.as_str())
        .bind(id.as_str())
        // No syntax is the empty string, not NULL: a bill we drafted ourselves
        // came from no file, and the column has always been NOT NULL.
        .bind(bill.source_syntax.map_or("", EInvoiceSyntax::as_str))
        .bind(&bill.source_sha256)
        .bind(bill.type_code)
        .bind(&bill.supplier.name)
        .bind(&bill.supplier.vat_id)
        .bind(&bill.supplier.legal_id)
        .bind(&bill.supplier.line1)
        .bind(&bill.supplier.line2)
        .bind(&bill.supplier.postal_code)
        .bind(&bill.supplier.city)
        .bind(&bill.supplier.country)
        .bind(&bill.supplier.email)
        .bind(&bill.supplier.iban)
        .bind(bill.supplier.key())
        .bind(&bill.number)
        .bind(bill.issue_date)
        .bind(bill.due_date)
        .bind(&bill.currency)
        .bind(&bill.buyer_reference)
        .bind(&bill.note)
        .bind(&bill.payment_reference)
        .bind(bill.totals.line_total_cents)
        .bind(bill.totals.allowance_total_cents)
        .bind(bill.totals.charge_total_cents)
        .bind(bill.totals.tax_exclusive_cents)
        .bind(bill.totals.tax_total_cents)
        .bind(bill.totals.tax_inclusive_cents)
        .bind(bill.totals.prepaid_cents)
        .bind(bill.totals.payable_cents)
        .bind(self.user.as_str())
        .execute(&mut **tx)
        .await
        .map_err(StoreError::Db)?;
        if inserted.rows_affected() == 0 {
            // The duplicate is reported as a conflict rather than swallowed: a
            // bookkeeper importing a file twice must be told it is already
            // there, or they will look for it in the wrong place.
            return Err(StoreError::Conflict(
                "this supplier's document with this number has already been imported".to_owned(),
            ));
        }

        for (index, line) in lines.iter().enumerate() {
            let order = i32::try_from(index)
                .map_err(|_| StoreError::Validation("a bill has too many lines".to_owned()))?;
            BILL_LINES
                .write(tx, self.tenant.as_str(), id.as_str(), order, line)
                .await?;
        }
        Ok(id)
    }

    /// This tenant's bills, newest document first, optionally only those in one
    /// status.
    ///
    /// # Errors
    /// [`StoreError::Db`] on failure.
    pub async fn billing_bills(&self, status: Option<BillStatus>) -> Result<Vec<Bill>> {
        let rows = sqlx::query_as::<_, BillRow>(&format!(
            "SELECT {BILL_COLS} FROM billing_bills \
             WHERE tenant_id = $1 AND ($2::text IS NULL OR status = $2) \
             ORDER BY issue_date DESC, imported_at DESC, id"
        ))
        .bind(self.tenant.as_str())
        .bind(status.map(BillStatus::as_str))
        .fetch_all(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        rows.into_iter().map(BillRow::into_bill).collect()
    }

    /// One of this tenant's bills with its lines, or `None` when the id is
    /// absent **or another tenant's** — never an existence oracle.
    ///
    /// # Errors
    /// [`StoreError::Db`] on failure.
    pub async fn billing_bill(&self, id: &BillingBillId) -> Result<Option<BillDocument>> {
        let row = sqlx::query_as::<_, BillRow>(&format!(
            "SELECT {BILL_COLS} FROM billing_bills WHERE tenant_id = $1 AND id = $2"
        ))
        .bind(self.tenant.as_str())
        .bind(id.as_str())
        .fetch_optional(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        let Some(row) = row else {
            return Ok(None);
        };
        let bill = row.into_bill()?;
        let lines = BILL_LINES
            .read(&self.pool, self.tenant.as_str(), id.as_str())
            .await?;
        let computed = totals(&lines.iter().map(Line::figures).collect::<Vec<_>>());
        Ok(Some(BillDocument {
            bill,
            lines,
            computed,
        }))
    }

    /// Approves or rejects one of this tenant's bills.
    ///
    /// The decision is taken under the bill's row lock and only from
    /// `received`, so two approvals racing each other cannot both write, and a
    /// decision already made is never quietly replaced by a second one. Who
    /// decided and when are stamped from the account handle and the database's
    /// clock — never from the caller.
    ///
    /// # Errors
    /// [`StoreError::NotFound`] when the id is absent or another tenant's;
    /// [`StoreError::Validation`] when the caller asks for `received`, which is
    /// not a decision; [`StoreError::Conflict`] when a decision has already been
    /// made; [`StoreError::Db`] on failure.
    pub async fn decide_billing_bill(
        &self,
        id: &BillingBillId,
        decision: BillStatus,
    ) -> Result<()> {
        if !decision.is_decided() {
            return Err(StoreError::Validation(
                "a decision on a bill is either approved or rejected".to_owned(),
            ));
        }
        let mut tx = self.pool.begin().await.map_err(StoreError::Db)?;
        let status = self.lock_bill(&mut tx, id).await?;
        if status.is_decided() {
            return Err(StoreError::Conflict(format!(
                "this bill has already been {}; a decision on a bill is final, and a bill \
                 accepted by mistake is corrected by the supplier's credit note",
                status.as_str()
            )));
        }
        sqlx::query(
            "UPDATE billing_bills SET status = $3, decided_by = $4, decided_at = now() \
             WHERE tenant_id = $1 AND id = $2",
        )
        .bind(self.tenant.as_str())
        .bind(id.as_str())
        .bind(decision.as_str())
        .bind(self.user.as_str())
        .execute(&mut *tx)
        .await
        .map_err(StoreError::Db)?;
        tx.commit().await.map_err(StoreError::Db)?;
        Ok(())
    }

    /// Deletes one of this tenant's bills, while nobody has decided about it.
    ///
    /// The undo for an import that should not have happened — the wrong file,
    /// somebody else's invoice. A **decided** bill is not deletable: an
    /// approved one is a liability the accounts carry, and a rejected one is
    /// the record of a refusal, which is exactly the thing a supplier will
    /// later dispute. Its lines go with it by cascade.
    ///
    /// # Errors
    /// [`StoreError::NotFound`] when the id is absent or another tenant's;
    /// [`StoreError::Conflict`] when a decision has been made;
    /// [`StoreError::Db`] on failure.
    pub async fn delete_billing_bill(&self, id: &BillingBillId) -> Result<()> {
        let mut tx = self.pool.begin().await.map_err(StoreError::Db)?;
        let status = self.lock_bill(&mut tx, id).await?;
        if status.is_decided() {
            return Err(StoreError::Conflict(format!(
                "this bill has been {} and is part of the record; it cannot be deleted",
                status.as_str()
            )));
        }
        sqlx::query("DELETE FROM billing_bills WHERE tenant_id = $1 AND id = $2")
            .bind(self.tenant.as_str())
            .bind(id.as_str())
            .execute(&mut *tx)
            .await
            .map_err(StoreError::Db)?;
        tx.commit().await.map_err(StoreError::Db)?;
        Ok(())
    }

    /// Takes the bill's row lock inside `tx` and returns its status.
    ///
    /// # Errors
    /// [`StoreError::NotFound`] when the id is absent **or another tenant's**;
    /// [`StoreError::Db`] on failure or on a status the code does not know.
    async fn lock_bill(
        &self,
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
        id: &BillingBillId,
    ) -> Result<BillStatus> {
        let row: Option<(String,)> = sqlx::query_as(
            "SELECT status FROM billing_bills WHERE tenant_id = $1 AND id = $2 FOR UPDATE",
        )
        .bind(self.tenant.as_str())
        .bind(id.as_str())
        .fetch_optional(&mut **tx)
        .await
        .map_err(StoreError::Db)?;
        let (status,) = row.ok_or(StoreError::NotFound)?;
        BillStatus::parse(&status).ok_or_else(|| {
            StoreError::Db(sqlx::Error::Decode(
                "billing_bills.status is not a known status".into(),
            ))
        })
    }
}

// ---- validation --------------------------------------------------------------

/// A validated, normalised bill ready to be bound into a statement.
#[derive(Debug)]
struct NormalizedBill {
    source_syntax: Option<EInvoiceSyntax>,
    source_sha256: String,
    type_code: &'static str,
    supplier: Supplier,
    number: String,
    issue_date: Date,
    due_date: Option<Date>,
    currency: String,
    buyer_reference: String,
    note: String,
    payment_reference: String,
    totals: BillTotals,
}

/// Validates and normalises a bill. Pure — no database, so the rules are
/// unit-tested directly, and one function runs for every door into the table.
fn normalize_bill(input: &NewBill) -> Result<NormalizedBill> {
    let supplier = Supplier {
        name: required(
            "supplier name",
            &input.supplier.name,
            SUPPLIER_NAME_MAX_CHARS,
        )?,
        vat_id: bounded("supplier VAT id", &input.supplier.vat_id, 40)?,
        legal_id: bounded("supplier registration", &input.supplier.legal_id, 60)?,
        line1: bounded("supplier address", &input.supplier.line1, 120)?,
        line2: bounded("supplier address", &input.supplier.line2, 120)?,
        postal_code: bounded("supplier post code", &input.supplier.postal_code, 20)?,
        city: bounded("supplier city", &input.supplier.city, 80)?,
        // Blank is accepted here, unlike on a customer: the standard requires a
        // supplier country (BR-08) and we require it of ourselves when we
        // *write* an e-invoice, but refusing to file an otherwise readable
        // invoice because a supplier's system left it out would help nobody.
        country: supplier_country(&input.supplier.country)?,
        email: bounded("supplier email", &input.supplier.email, 200)?,
        iban: bounded("supplier IBAN", &input.supplier.iban, 40)?,
    };
    let source_sha256 = source_checksum(input.source_syntax, &input.source_sha256)?;
    let issue_date = input.issue_date.ok_or_else(|| {
        StoreError::Validation("a bill must carry the date the supplier issued it".to_owned())
    })?;
    if let Some(due) = input.due_date
        && due < issue_date
    {
        return Err(StoreError::Validation(
            "a bill cannot be due before it was issued".to_owned(),
        ));
    }

    Ok(NormalizedBill {
        source_syntax: input.source_syntax,
        source_sha256,
        type_code: if input.credit_note { "381" } else { "380" },
        supplier,
        number: required("document number", &input.number, BILL_NUMBER_MAX_CHARS)?,
        issue_date,
        due_date: input.due_date,
        currency: currency(&input.currency)?,
        buyer_reference: bounded(
            "buyer reference",
            &input.buyer_reference,
            BILL_TEXT_MAX_CHARS,
        )?,
        note: bounded("note", &input.note, BILL_TEXT_MAX_CHARS)?,
        payment_reference: bounded(
            "payment reference",
            &input.payment_reference,
            BILL_TEXT_MAX_CHARS,
        )?,
        totals: checked_totals(input.totals)?,
    })
}

/// The supplier's country: blank when they stated none, and otherwise held to
/// the same two-letter shape as everybody else's.
fn supplier_country(value: &str) -> Result<String> {
    if value.trim().is_empty() {
        return Ok(String::new());
    }
    country(value)
}

/// Bounds every stated total, so a document claiming an absurd figure is
/// refused rather than stored and summed into a report later.
fn checked_totals(totals: BillTotals) -> Result<BillTotals> {
    for (name, value) in [
        ("line total", totals.line_total_cents),
        ("allowance total", totals.allowance_total_cents),
        ("charge total", totals.charge_total_cents),
        ("total without VAT", totals.tax_exclusive_cents),
        ("VAT total", totals.tax_total_cents),
        ("total with VAT", totals.tax_inclusive_cents),
        ("prepaid amount", totals.prepaid_cents),
        ("amount due", totals.payable_cents),
    ] {
        if !(-BILL_MAX_CENTS..=BILL_MAX_CENTS).contains(&value) {
            return Err(StoreError::Validation(format!(
                "the {name} is larger than any real invoice; the file is not one alo can store"
            )));
        }
    }
    Ok(totals)
}

/// The checksum a bill records of the file it was read from — and **no**
/// checksum for a bill that was read from no file.
///
/// A bill drafted from a goods receipt ([`crate::inv_po_receive`]) states what
/// we ordered and received; there is no document to hash, and inventing a hash
/// of our own bytes would claim a provenance the record does not have. So a
/// bill with no syntax carries an empty checksum, and one that names a syntax
/// must carry a real hex SHA-256 — the value that ties it to the archived file.
///
/// # Errors
/// [`StoreError::Validation`] when a bill from a file has no usable checksum,
/// or a bill from no file claims one.
fn source_checksum(syntax: Option<EInvoiceSyntax>, value: &str) -> Result<String> {
    if syntax.is_none() {
        if value.is_empty() {
            return Ok(String::new());
        }
        return Err(StoreError::Validation(
            "a bill that was read from no file cannot carry a file's checksum".to_owned(),
        ));
    }
    hex_sha256(value)
}

/// Checks a hex SHA-256, which is written by us and never by a caller: a
/// malformed one means a bug here, so it is refused rather than stored.
fn hex_sha256(value: &str) -> Result<String> {
    let ok = value.len() == 64
        && value
            .chars()
            .all(|c| c.is_ascii_digit() || matches!(c, 'a'..='f'));
    if ok {
        Ok(value.to_owned())
    } else {
        Err(StoreError::Validation(
            "a bill must record the checksum of the file it was read from".to_owned(),
        ))
    }
}

/// The SHA-256 of the imported bytes, lower-case hex.
fn sha256_hex(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    digest.iter().map(|byte| format!("{byte:02x}")).collect()
}

/// The bill a parsed e-invoice becomes.
///
/// A straight carry-over: every decision about what the document *means* was
/// made by the parser and the standard's rules, and this function only moves
/// values into the shape the table holds.
fn new_bill_from(invoice: &InboundInvoice, sha256: &str) -> NewBill {
    NewBill {
        source_syntax: Some(invoice.syntax),
        source_sha256: sha256.to_owned(),
        credit_note: invoice.credit_note,
        supplier: Supplier {
            name: invoice.seller.name.clone(),
            vat_id: invoice.seller.vat_id.clone(),
            legal_id: invoice.seller.legal_id.clone(),
            line1: invoice.seller.line1.clone(),
            line2: invoice.seller.line2.clone(),
            postal_code: invoice.seller.postal_code.clone(),
            city: invoice.seller.city.clone(),
            country: invoice.seller.country.clone(),
            email: invoice.seller.email.clone(),
            iban: invoice.iban.clone(),
        },
        number: invoice.number.clone(),
        issue_date: Some(invoice.issue_date),
        due_date: invoice.due_date,
        currency: invoice.currency.clone(),
        buyer_reference: invoice.buyer_reference.clone(),
        note: invoice.note.clone(),
        payment_reference: invoice.payment_reference.clone(),
        totals: BillTotals {
            line_total_cents: invoice.totals.line_total_cents,
            allowance_total_cents: invoice.totals.allowance_total_cents,
            charge_total_cents: invoice.totals.charge_total_cents,
            tax_exclusive_cents: invoice.totals.tax_exclusive_cents,
            tax_total_cents: invoice.totals.tax_total_cents,
            tax_inclusive_cents: invoice.totals.tax_inclusive_cents,
            prepaid_cents: invoice.totals.prepaid_cents,
            payable_cents: invoice.totals.payable_cents,
        },
        lines: invoice
            .lines
            .iter()
            .map(|line| NewLine {
                description: line.description.clone(),
                unit: line.unit.clone(),
                qty_milli: line.qty_milli,
                unit_price_cents: line.unit_price_cents,
                vat_rate_bp: line.vat_rate_bp,
            })
            .collect(),
    }
}

// ---- row types ----------------------------------------------------------------

/// One row of `billing_bills`, in [`BILL_COLS`] order. `pub(crate)` for the
/// same one sibling that shares the column list.
#[derive(sqlx::FromRow)]
pub(crate) struct BillRow {
    id: String,
    source_syntax: String,
    source_sha256: String,
    type_code: String,
    status: String,
    supplier_name: String,
    supplier_vat_id: String,
    supplier_legal_id: String,
    supplier_line1: String,
    supplier_line2: String,
    supplier_postal_code: String,
    supplier_city: String,
    supplier_country: String,
    supplier_email: String,
    supplier_iban: String,
    number: String,
    issue_date: Date,
    due_date: Option<Date>,
    currency: String,
    buyer_reference: String,
    note: String,
    payment_reference: String,
    line_total_cents: i64,
    allowance_total_cents: i64,
    charge_total_cents: i64,
    tax_exclusive_cents: i64,
    tax_total_cents: i64,
    tax_inclusive_cents: i64,
    prepaid_cents: i64,
    payable_cents: i64,
    imported_by: String,
    imported_at: OffsetDateTime,
    decided_by: Option<String>,
    decided_at: Option<OffsetDateTime>,
    exported_at: Option<OffsetDateTime>,
    exported_by: Option<String>,
    export_message_id: Option<String>,
}

impl BillRow {
    /// The stored bill.
    ///
    /// # Errors
    /// [`StoreError::Db`] when the row carries a status the code does not know
    /// — a decode failure rather than a guess, because guessing here would
    /// decide whether a liability is approved.
    pub(crate) fn into_bill(self) -> Result<Bill> {
        let status = BillStatus::parse(&self.status).ok_or_else(|| {
            StoreError::Db(sqlx::Error::Decode(
                "billing_bills.status is not a known status".into(),
            ))
        })?;
        Ok(Bill {
            id: BillingBillId::new(self.id),
            source_syntax: EInvoiceSyntax::parse(&self.source_syntax),
            source_sha256: self.source_sha256,
            credit_note: self.type_code == "381",
            status,
            supplier: Supplier {
                name: self.supplier_name,
                vat_id: self.supplier_vat_id,
                legal_id: self.supplier_legal_id,
                line1: self.supplier_line1,
                line2: self.supplier_line2,
                postal_code: self.supplier_postal_code,
                city: self.supplier_city,
                country: self.supplier_country,
                email: self.supplier_email,
                iban: self.supplier_iban,
            },
            number: self.number,
            issue_date: self.issue_date,
            due_date: self.due_date,
            currency: self.currency,
            buyer_reference: self.buyer_reference,
            note: self.note,
            payment_reference: self.payment_reference,
            totals: BillTotals {
                line_total_cents: self.line_total_cents,
                allowance_total_cents: self.allowance_total_cents,
                charge_total_cents: self.charge_total_cents,
                tax_exclusive_cents: self.tax_exclusive_cents,
                tax_total_cents: self.tax_total_cents,
                tax_inclusive_cents: self.tax_inclusive_cents,
                prepaid_cents: self.prepaid_cents,
                payable_cents: self.payable_cents,
            },
            imported_by: self.imported_by,
            imported_at: self.imported_at,
            decided_by: self.decided_by,
            decided_at: self.decided_at,
            exported_at: self.exported_at,
            exported_by: self.exported_by,
            export_message_id: self.export_message_id,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use time::Month;

    fn day(year: i32, month: u8, day: u8) -> Date {
        Date::from_calendar_date(year, Month::try_from(month).unwrap_or(Month::January), day)
            .unwrap_or(Date::MIN)
    }

    fn bill() -> NewBill {
        NewBill {
            source_syntax: Some(EInvoiceSyntax::Cii),
            source_sha256: "a".repeat(64),
            credit_note: false,
            supplier: Supplier {
                name: " Lieferant GmbH ".to_owned(),
                vat_id: "DE811907980".to_owned(),
                country: "de".to_owned(),
                ..Supplier::default()
            },
            number: " R-2026-77 ".to_owned(),
            issue_date: Some(day(2026, 8, 7)),
            due_date: Some(day(2026, 8, 21)),
            currency: "eur".to_owned(),
            totals: BillTotals {
                line_total_cents: 100_000,
                tax_exclusive_cents: 100_000,
                tax_total_cents: 21_000,
                tax_inclusive_cents: 121_000,
                payable_cents: 121_000,
                ..BillTotals::default()
            },
            lines: vec![NewLine {
                description: "Consulting".to_owned(),
                unit: "hour".to_owned(),
                qty_milli: 8_000,
                unit_price_cents: 12_500,
                vat_rate_bp: 2100,
            }],
            ..NewBill::default()
        }
    }

    fn refused(input: &NewBill) -> String {
        match normalize_bill(input) {
            Err(StoreError::Validation(message)) => message,
            other => panic!("expected a Validation refusal, got {other:?}"),
        }
    }

    #[test]
    fn a_bill_is_normalised_the_way_every_other_billing_record_is() {
        let bill = normalize_bill(&bill()).unwrap_or_else(|e| panic!("rejected: {e}"));
        assert_eq!(bill.supplier.name, "Lieferant GmbH", "trimmed");
        assert_eq!(bill.supplier.country, "DE", "uppercased");
        assert_eq!(bill.currency, "EUR");
        assert_eq!(bill.number, "R-2026-77");
        assert_eq!(bill.type_code, "380");
        assert_eq!(bill.issue_date, day(2026, 8, 7));
    }

    #[test]
    fn a_credit_note_is_stored_as_the_type_it_is() {
        let credit = NewBill {
            credit_note: true,
            ..bill()
        };
        let normalized = normalize_bill(&credit).unwrap_or_else(|e| panic!("rejected: {e}"));
        assert_eq!(normalized.type_code, "381");
    }

    #[test]
    fn a_bill_must_name_its_supplier_its_number_and_its_date() {
        assert!(
            refused(&NewBill {
                supplier: Supplier {
                    name: "   ".to_owned(),
                    ..bill().supplier
                },
                ..bill()
            })
            .contains("supplier name")
        );
        assert!(
            refused(&NewBill {
                number: String::new(),
                ..bill()
            })
            .contains("document number")
        );
        assert!(
            refused(&NewBill {
                issue_date: None,
                ..bill()
            })
            .contains("date")
        );
    }

    #[test]
    fn a_bill_cannot_be_due_before_it_was_issued() {
        let message = refused(&NewBill {
            due_date: Some(day(2026, 7, 1)),
            ..bill()
        });
        assert!(message.contains("due before"), "{message}");
        // The same day is fine: payable on receipt is an ordinary term.
        assert!(
            normalize_bill(&NewBill {
                due_date: Some(day(2026, 8, 7)),
                ..bill()
            })
            .is_ok()
        );
        // And no due date at all is fine: many invoices state only terms.
        assert!(
            normalize_bill(&NewBill {
                due_date: None,
                ..bill()
            })
            .is_ok()
        );
    }

    #[test]
    fn an_absurd_total_is_refused_before_it_reaches_a_report() {
        for totals in [
            BillTotals {
                payable_cents: BILL_MAX_CENTS + 1,
                ..bill().totals
            },
            BillTotals {
                line_total_cents: -BILL_MAX_CENTS - 1,
                ..bill().totals
            },
            BillTotals {
                tax_total_cents: i64::MAX,
                ..bill().totals
            },
        ] {
            assert!(refused(&NewBill { totals, ..bill() }).contains("larger than any real"));
        }
        // A credit note's negative totals are ordinary.
        assert!(
            normalize_bill(&NewBill {
                credit_note: true,
                totals: BillTotals {
                    line_total_cents: -100_000,
                    tax_exclusive_cents: -100_000,
                    tax_total_cents: -21_000,
                    tax_inclusive_cents: -121_000,
                    payable_cents: -121_000,
                    ..BillTotals::default()
                },
                ..bill()
            })
            .is_ok()
        );
    }

    #[test]
    fn the_supplier_key_is_the_vat_id_when_there_is_one_and_the_name_otherwise() {
        let with_vat = Supplier {
            name: "Lieferant GmbH".to_owned(),
            vat_id: "de811907980".to_owned(),
            ..Supplier::default()
        };
        assert_eq!(with_vat.key(), "DE811907980");
        // A supplier who renames themselves keeps the same key…
        let renamed = Supplier {
            name: "Lieferant Holding GmbH".to_owned(),
            ..with_vat.clone()
        };
        assert_eq!(renamed.key(), with_vat.key());
        // …and one with no VAT id is keyed by their name, case-folded.
        let no_vat = Supplier {
            name: " Sole Trader ".to_owned(),
            vat_id: String::new(),
            ..Supplier::default()
        };
        assert_eq!(no_vat.key(), "sole trader");
    }

    #[test]
    fn a_checksum_is_recorded_and_a_malformed_one_is_a_bug_we_refuse() {
        assert_eq!(sha256_hex(b"").len(), 64);
        assert_eq!(
            sha256_hex(b"alo"),
            sha256_hex(b"alo"),
            "the same bytes hash the same"
        );
        assert_ne!(sha256_hex(b"alo"), sha256_hex(b"alo "));
        assert!(
            sha256_hex(b"alo")
                .chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit())
        );
        for bad in ["", "abc", &"A".repeat(64), &"z".repeat(64)] {
            assert!(
                refused(&NewBill {
                    source_sha256: bad.to_owned(),
                    ..bill()
                })
                .contains("checksum")
            );
        }
    }

    #[test]
    fn a_bill_read_from_no_file_carries_no_checksum_and_may_not_claim_one() {
        // The shape a goods receipt drafts (B5.05b): our own statement of what
        // we ordered and received, with no supplier document behind it.
        let ours = NewBill {
            source_syntax: None,
            source_sha256: String::new(),
            ..bill()
        };
        let normalized = normalize_bill(&ours).unwrap_or_else(|e| panic!("rejected: {e}"));
        assert!(normalized.source_syntax.is_none());
        assert!(normalized.source_sha256.is_empty());

        let claiming = NewBill {
            source_syntax: None,
            source_sha256: "a".repeat(64),
            ..bill()
        };
        assert!(refused(&claiming).contains("read from no file"));
    }

    #[test]
    fn every_status_has_one_stable_name_and_only_a_decision_is_a_decision() {
        for status in [
            BillStatus::Received,
            BillStatus::Approved,
            BillStatus::Rejected,
        ] {
            assert_eq!(BillStatus::parse(status.as_str()), Some(status));
        }
        assert_eq!(BillStatus::parse("paid"), None);
        assert!(!BillStatus::Received.is_decided());
        assert!(BillStatus::Approved.is_decided());
        assert!(BillStatus::Rejected.is_decided());
    }
}
