//! The journal — what a document did to the books (ADR 0035, wave B4;
//! `docs/design/finance.md`, "The journal").
//!
//! Every figure alo Finance will ever report is a fold over the two tables
//! this module writes, and three rules make that safe to build reports on:
//!
//! - **An entry is written whole, in one transaction, by one function
//!   ([`AccountStore::post_fin_entry`]), and is never updated or deleted.**
//!   There is no API here to add a posting to an existing entry and none to
//!   change one. That is what makes the balance invariant enforceable at all:
//!   an entry that can never be edited can only be unbalanced at the instant it
//!   is written, so one check in one place covers every path forever. A
//!   correction is a **reversal** — a mirror entry carrying
//!   [`NewEntry::reverses_entry_id`], dated on or after the original — which is
//!   what an auditor expects to see and what a void or a credit note already is
//!   one layer up.
//! - **One signed amount: positive debits, negative credits, `Σ = 0`.** In both
//!   money columns — the document's own currency and the tenant's accounting
//!   currency — because an entry that balances in euro and not in dollars is
//!   two different stories about one event. The debit/credit words survive
//!   where humans read them: a journal screen renders two columns from the
//!   sign.
//! - **A document posts exactly once.** `UNIQUE (tenant_id, source_kind,
//!   source_id, source_event)` is the whole mechanism: issuing invoice X posts
//!   `('invoice', X, 'issue')`, and a retry, a double-click or a re-run of the
//!   backfill is a typed [`StoreError::Conflict`] rather than a second set of
//!   postings.
//!
//! What is deliberately **not** here: the rules that decide *which* accounts a
//! document touches. Those are pure functions over a document (`fin_rules`,
//! B4.04), unit-testable against a hand-written golden before they are ever
//! wired into a transaction; this file owns only writing what they return and
//! reading it back. The rounding and exchange-difference postings the note
//! describes are produced there too — this module's job is to refuse the entry
//! if they are missing or wrong.

use time::{Date, OffsetDateTime};

use crate::account::AccountStore;
use crate::billing_field::{bounded, currency as currency_code, vat_rate_bp};
use crate::billing_fx::{FxSnapshot, IDENTITY_RATE_MICRO, rate_micro};
use crate::error::{Result, StoreError};
use crate::id::{FinAccountId, FinEntryId, FinPostingId};

/// The longest memo an entry or a posting may carry — a line on a journal
/// screen, not a document.
pub const MEMO_MAX_CHARS: usize = 500;
/// The longest a posting's dimension value (customer, supplier, project, user)
/// may be. Our own ids are 22 characters; a supplier key from an imported bill
/// is a name-shaped thing, and this is the same ceiling a customer name has.
pub const DIMENSION_MAX_CHARS: usize = 200;
/// The largest amount one posting may move, in cents: €10 000 000 000.00.
///
/// A typo guard with an arithmetic job, like
/// [`crate::billing_field::UNIT_PRICE_MAX_CENTS`]. With
/// [`ENTRY_POSTINGS_MAX`] postings of this size, the sums this module adds up
/// stay four orders of magnitude inside `i64`, so no entry can balance-check
/// against a wrapped number.
pub const POSTING_AMOUNT_MAX_CENTS: i64 = 1_000_000_000_000;
/// The most postings one entry may have. A 400-line invoice with a VAT posting
/// per rate is nowhere near it; a generated entry that runs away is.
pub const ENTRY_POSTINGS_MAX: usize = 1_000;
/// The most entries one journal read returns.
pub const JOURNAL_PAGE_MAX: i64 = 500;

/// The columns every read of an entry selects, in [`EntryRow`] order.
const ENTRY_COLS: &str = "id, entry_date, kind, source_kind, source_id, source_event, memo, \
     reverses_entry_id, attachment_node_id, currency, fx_base_currency, fx_rate_micro, \
     fx_rate_date, created_by, created_at";

/// The columns every read of a posting selects, in [`PostingRow`] order.
const POSTING_COLS: &str = "id, entry_id, position, account_id, amount_cents, base_cents, \
     vat_rate_bp, customer_id, supplier_key, project_id, user_id, memo";

/// What kind of event an entry books.
///
/// Closed, and it is *ours*: a wave that books a new kind of document adds a
/// variant here together with the posting rule that produces it, rather than a
/// caller inventing a word that no report knows how to group.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EntryKind {
    /// An invoice was issued (B1.08's gapless number is what makes it an
    /// event: before that it is an intention).
    Invoice,
    /// A credit note was issued against one.
    CreditNote,
    /// A customer's money arrived.
    Payment,
    /// A supplier's bill was approved.
    Bill,
    /// A supplier was paid.
    BillPayment,
    /// An employee's expense claim was approved.
    Expense,
    /// An employee was paid back for one.
    Reimbursement,
    /// A mileage claim was approved.
    Mileage,
    /// An accountant typed it.
    Manual,
    /// The balances a tenant arrived with, the day their books opened here.
    Opening,
    /// A mirror of an earlier entry — the only correction this module has.
    Reversal,
}

impl EntryKind {
    /// The stored word — the wire form and the database value, one spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Invoice => "invoice",
            Self::CreditNote => "credit_note",
            Self::Payment => "payment",
            Self::Bill => "bill",
            Self::BillPayment => "bill_payment",
            Self::Expense => "expense",
            Self::Reimbursement => "reimbursement",
            Self::Mileage => "mileage",
            Self::Manual => "manual",
            Self::Opening => "opening",
            Self::Reversal => "reversal",
        }
    }

    /// Every kind, in the order the note's table introduces them.
    pub const ALL: &'static [Self] = &[
        Self::Invoice,
        Self::CreditNote,
        Self::Payment,
        Self::Bill,
        Self::BillPayment,
        Self::Expense,
        Self::Reimbursement,
        Self::Mileage,
        Self::Manual,
        Self::Opening,
        Self::Reversal,
    ];

    /// Reads the stored word back.
    ///
    /// # Errors
    /// [`StoreError::Validation`] naming the accepted set when the word is not
    /// one of ours.
    pub fn parse(value: &str) -> Result<Self> {
        let value = value.trim();
        Self::ALL
            .iter()
            .copied()
            .find(|kind| kind.as_str() == value)
            .ok_or_else(|| {
                StoreError::Validation(format!(
                    "entry kind must be one of: {}",
                    words(Self::ALL.iter().map(|kind| kind.as_str()))
                ))
            })
    }
}

/// Which document produced an entry.
///
/// Closed for the same reason as [`EntryKind`]: the pair (kind, id) is what the
/// idempotency index is keyed on, so a caller inventing a word could post the
/// same document twice under two spellings.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SourceKind {
    /// A billing invoice or credit note (B1.06).
    Invoice,
    /// A customer payment (B1.19).
    Payment,
    /// A supplier bill (B1.24).
    Bill,
    /// An employee expense claim (B4.05).
    Expense,
    /// A line of an imported bank statement (B4.08).
    BankLine,
}

impl SourceKind {
    /// The stored word.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Invoice => "invoice",
            Self::Payment => "payment",
            Self::Bill => "bill",
            Self::Expense => "expense",
            Self::BankLine => "bank_line",
        }
    }

    /// Every source kind.
    pub const ALL: &'static [Self] = &[
        Self::Invoice,
        Self::Payment,
        Self::Bill,
        Self::Expense,
        Self::BankLine,
    ];

    /// Reads the stored word back.
    ///
    /// # Errors
    /// [`StoreError::Validation`] naming the accepted set.
    pub fn parse(value: &str) -> Result<Self> {
        let value = value.trim();
        Self::ALL
            .iter()
            .copied()
            .find(|kind| kind.as_str() == value)
            .ok_or_else(|| {
                StoreError::Validation(format!(
                    "source kind must be one of: {}",
                    words(Self::ALL.iter().map(|kind| kind.as_str()))
                ))
            })
    }
}

/// Which event of that document was booked.
///
/// The same document is posted more than once in its life — an invoice is
/// issued and may later be voided — and the event is what keeps those two
/// entries apart under one unique key.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SourceEvent {
    /// The document became irrevocable: an invoice got its number, a credit
    /// note was raised.
    Issue,
    /// It was taken back, and the entry that books it is a reversal.
    Void,
    /// Money moved against it.
    Settle,
    /// A bill or an expense claim was approved — the moment it becomes a debt.
    Approve,
    /// An employee was paid back.
    Reimburse,
}

impl SourceEvent {
    /// The stored word.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Issue => "issue",
            Self::Void => "void",
            Self::Settle => "settle",
            Self::Approve => "approve",
            Self::Reimburse => "reimburse",
        }
    }

    /// Every event.
    pub const ALL: &'static [Self] = &[
        Self::Issue,
        Self::Void,
        Self::Settle,
        Self::Approve,
        Self::Reimburse,
    ];

    /// Reads the stored word back.
    ///
    /// # Errors
    /// [`StoreError::Validation`] naming the accepted set.
    pub fn parse(value: &str) -> Result<Self> {
        let value = value.trim();
        Self::ALL
            .iter()
            .copied()
            .find(|event| event.as_str() == value)
            .ok_or_else(|| {
                StoreError::Validation(format!(
                    "source event must be one of: {}",
                    words(Self::ALL.iter().map(|event| event.as_str()))
                ))
            })
    }
}

/// Joins a closed set's words for an error message that tells the caller what
/// would have worked.
fn words<'a>(items: impl Iterator<Item = &'a str>) -> String {
    items.collect::<Vec<_>>().join(", ")
}

/// The document event an entry books — all three parts or none of them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EntrySource {
    /// Which kind of document.
    pub kind: SourceKind,
    /// Its id, in whatever module owns it.
    pub id: String,
    /// Which of its events.
    pub event: SourceEvent,
}

/// One line of an entry, as a posting rule hands it over.
#[derive(Debug, Clone)]
pub struct NewPosting {
    /// The account it moves, resolved by the rule through
    /// [`AccountStore::fin_account_for_role`] — never by code.
    pub account_id: FinAccountId,
    /// Signed, in the entry's own currency: positive debits, negative credits.
    pub amount_cents: i64,
    /// The same money in the tenant's accounting currency, crossed at the
    /// entry's snapshot rate. Equal to `amount_cents` when the document is
    /// already in that currency.
    pub base_cents: i64,
    /// Which VAT rate this posting's tax belongs to, for the postings that
    /// carry tax.
    pub vat_rate_bp: Option<i32>,
    /// Who owed it (a billing customer).
    pub customer_id: Option<String>,
    /// Whose bill it was (a supplier key, as `billing_bills` holds one).
    pub supplier_key: Option<String>,
    /// Which engagement it belongs to (a project).
    pub project_id: Option<String>,
    /// Whose expense it was (a user).
    pub user_id: Option<String>,
    /// A line of explanation for a human reading the entry.
    pub memo: String,
}

impl NewPosting {
    /// A posting with no dimensions and no memo — the shape most rules start
    /// from, filled in with the ones they actually have.
    pub fn new(account_id: FinAccountId, amount_cents: i64, base_cents: i64) -> Self {
        Self {
            account_id,
            amount_cents,
            base_cents,
            vat_rate_bp: None,
            customer_id: None,
            supplier_key: None,
            project_id: None,
            user_id: None,
            memo: String::new(),
        }
    }
}

/// A whole entry, as it is posted: written once, in one transaction, or not at
/// all.
#[derive(Debug, Clone)]
pub struct NewEntry {
    /// The accounting date — the *document's* date, never today
    /// (`docs/design/finance.md`: a ledger keyed on when a clerk typed is a
    /// ledger no period report can trust).
    pub entry_date: Date,
    /// What kind of event this is.
    pub kind: EntryKind,
    /// Which document event produced it, or `None` for a manual entry.
    pub source: Option<EntrySource>,
    /// What a human reading the journal should see.
    pub memo: String,
    /// The entry this one corrects, if it is a reversal.
    pub reverses_entry_id: Option<FinEntryId>,
    /// A Drive node holding the evidence, for a manual entry.
    pub attachment_node_id: Option<String>,
    /// The currency the document is denominated in.
    pub currency: String,
    /// The rate the amounts were restated into the tenant's accounting
    /// currency at, frozen on this entry (B1.21).
    pub fx: FxSnapshot,
    /// The lines. At least two, and they must add to zero in both columns.
    pub postings: Vec<NewPosting>,
}

/// A stored entry.
#[derive(Debug, Clone)]
pub struct Entry {
    /// Opaque id, unique within the tenant.
    pub id: FinEntryId,
    /// The accounting date.
    pub entry_date: Date,
    /// What kind of event it books.
    pub kind: EntryKind,
    /// The document event it books, or `None` for a manual entry.
    pub source: Option<EntrySource>,
    /// What a human reading the journal sees.
    pub memo: String,
    /// The entry this one corrects, if any.
    pub reverses_entry_id: Option<FinEntryId>,
    /// The Drive node holding the evidence, if any.
    pub attachment_node_id: Option<String>,
    /// The currency the document was denominated in.
    pub currency: String,
    /// The rate snapshot the base amounts were computed at.
    pub fx: FxSnapshot,
    /// The user whose action posted it.
    pub created_by: String,
    /// When it was posted (which is not the accounting date).
    pub created_at: OffsetDateTime,
}

/// A stored posting.
#[derive(Debug, Clone)]
pub struct Posting {
    /// Opaque id, unique within the tenant.
    pub id: FinPostingId,
    /// The entry it belongs to.
    pub entry_id: FinEntryId,
    /// Its place in the entry, from zero.
    pub position: i32,
    /// The account it moves.
    pub account_id: FinAccountId,
    /// Signed, in the entry's currency: positive debits, negative credits.
    pub amount_cents: i64,
    /// The same money in the tenant's accounting currency.
    pub base_cents: i64,
    /// The VAT rate this posting's tax belongs to, if any.
    pub vat_rate_bp: Option<i32>,
    /// Who owed it.
    pub customer_id: Option<String>,
    /// Whose bill it was.
    pub supplier_key: Option<String>,
    /// Which engagement it belongs to.
    pub project_id: Option<String>,
    /// Whose expense it was.
    pub user_id: Option<String>,
    /// The line of explanation.
    pub memo: String,
}

impl Posting {
    /// The debit column a human reads: the amount when it is a debit, else
    /// zero. Stated once here so a screen, a CSV and a report cannot disagree
    /// about which side a posting is on.
    pub fn debit_cents(&self) -> i64 {
        self.amount_cents.max(0)
    }

    /// The credit column, as a positive number.
    pub fn credit_cents(&self) -> i64 {
        (-self.amount_cents).max(0)
    }
}

/// An entry with its postings, in the order they were written — how the
/// journal is read.
#[derive(Debug, Clone)]
pub struct JournalEntry {
    /// The entry header.
    pub entry: Entry,
    /// Its postings, ordered by [`Posting::position`].
    pub postings: Vec<Posting>,
}

/// The **mirror** of an entry: the same postings with both money columns
/// negated, carrying [`EntryKind::Reversal`] and pointing at what it corrects.
///
/// This is the only correction the books have. Nothing here is ever updated or
/// deleted (`docs/design/finance.md`, "Signed amounts"; migration 0131), so an
/// act taken back leaves **two** entries a reader can see rather than one that
/// quietly changed — which is the difference between a ledger and a spreadsheet.
///
/// Three readings, each of which would be a bug taken the other way:
///
/// - **The dimensions are kept, not dropped.** A relief posted against a
///   customer has to be *un*-relieved against that same customer, or the aged
///   debtors report keeps a balance no document explains.
/// - **The date is the original's.** A correction belongs in the period the
///   thing it corrects moved money in; dating it today would take money out of a
///   period that was already reported. (When B4.10 locks that period, the
///   reversal is refused rather than re-dated — a locked period is exactly the
///   case where a person has to decide, and `post_fin_entry_in` already refuses
///   a reversal dated before its original.)
/// - **The rate is the original's snapshot**, not today's: reversing an entry
///   at a different rate would leave an exchange difference nobody made.
///
/// `source` is the event the reversal itself books — a payment taken back is
/// `(payment, its id, void)` — or `None` for a correction that belongs to no
/// document. It is never the original's own source, which is already taken
/// (`fin_entries_source_once`).
#[must_use]
pub fn reversal_entry(original: &JournalEntry, source: Option<EntrySource>) -> NewEntry {
    NewEntry {
        entry_date: original.entry.entry_date,
        kind: EntryKind::Reversal,
        source,
        memo: original.entry.memo.clone(),
        reverses_entry_id: Some(original.entry.id.clone()),
        attachment_node_id: None,
        currency: original.entry.currency.clone(),
        fx: original.entry.fx.clone(),
        postings: original
            .postings
            .iter()
            .map(|posting| NewPosting {
                account_id: posting.account_id.clone(),
                // Bounded by the money CHECK every alo column carries, so the
                // saturation can never bite; it is here because a panic in the
                // books is not a correction.
                amount_cents: posting.amount_cents.saturating_neg(),
                base_cents: posting.base_cents.saturating_neg(),
                vat_rate_bp: posting.vat_rate_bp,
                customer_id: posting.customer_id.clone(),
                supplier_key: posting.supplier_key.clone(),
                project_id: posting.project_id.clone(),
                user_id: posting.user_id.clone(),
                memo: posting.memo.clone(),
            })
            .collect(),
    }
}

/// A validated, normalised entry ready to be bound into statements.
#[derive(Debug)]
struct Normalized {
    entry_date: Date,
    kind: &'static str,
    source_kind: String,
    source_id: String,
    source_event: String,
    memo: String,
    reverses_entry_id: Option<String>,
    attachment_node_id: Option<String>,
    currency: String,
    fx_base_currency: String,
    fx_rate_micro: i64,
    fx_rate_date: Date,
    postings: Vec<NormalizedPosting>,
}

/// A validated, normalised posting.
#[derive(Debug)]
struct NormalizedPosting {
    account_id: String,
    amount_cents: i64,
    base_cents: i64,
    vat_rate_bp: Option<i32>,
    customer_id: Option<String>,
    supplier_key: Option<String>,
    project_id: Option<String>,
    user_id: Option<String>,
    memo: String,
}

/// Trims an optional dimension and treats blank as absent: a stored empty
/// string would be a dimension a report groups by and nobody set.
fn dimension(field: &str, value: Option<&String>) -> Result<Option<String>> {
    match value {
        None => Ok(None),
        Some(raw) => {
            let trimmed = bounded(field, raw, DIMENSION_MAX_CHARS)?;
            Ok((!trimmed.is_empty()).then_some(trimmed))
        }
    }
}

/// Validates one amount against [`POSTING_AMOUNT_MAX_CENTS`], in either
/// direction — a posting is signed, so the ceiling is on its magnitude.
fn amount_cents(field: &str, value: i64) -> Result<i64> {
    if !(-POSTING_AMOUNT_MAX_CENTS..=POSTING_AMOUNT_MAX_CENTS).contains(&value) {
        return Err(StoreError::Validation(format!(
            "{field} must be between -{POSTING_AMOUNT_MAX_CENTS} and \
             {POSTING_AMOUNT_MAX_CENTS} cents"
        )));
    }
    Ok(value)
}

/// Adds one posting's amount into a running sum, refusing an overflow rather
/// than balance-checking against a wrapped number.
fn accumulate(running: i64, value: i64) -> Result<i64> {
    running.checked_add(value).ok_or_else(|| {
        StoreError::Validation("the entry's amounts are too large to add up".to_owned())
    })
}

/// Validates and normalises a whole entry, **including the balance rule**.
/// Pure — no database, no clock — so every rule below is unit-tested directly
/// and the write path can be read as "check, then write".
fn normalize(input: &NewEntry) -> Result<Normalized> {
    if input.postings.len() < 2 {
        return Err(StoreError::Validation(
            "a journal entry needs at least two postings: one account is not double-entry"
                .to_owned(),
        ));
    }
    if input.postings.len() > ENTRY_POSTINGS_MAX {
        return Err(StoreError::Validation(format!(
            "a journal entry may have at most {ENTRY_POSTINGS_MAX} postings"
        )));
    }

    let currency = currency_code(&input.currency)?;
    let fx_base_currency = currency_code(&input.fx.base_currency)?;
    let fx_rate_micro = rate_micro(input.fx.rate_micro)?;
    // A document raised in the currency the books are kept in converts at the
    // identity rate; anything else is a rate applied to itself, and it would
    // make the base column a different number from the document column for no
    // reason a reader could ever reconstruct.
    if currency == fx_base_currency && fx_rate_micro != IDENTITY_RATE_MICRO {
        return Err(StoreError::Validation(
            "an entry in the accounting currency must carry the identity exchange rate".to_owned(),
        ));
    }

    let (source_kind, source_id, source_event) = match &input.source {
        None => (String::new(), String::new(), String::new()),
        Some(source) => {
            let id = bounded("the source document id", &source.id, DIMENSION_MAX_CHARS)?;
            if id.is_empty() {
                return Err(StoreError::Validation(
                    "a source document id must not be empty".to_owned(),
                ));
            }
            (
                source.kind.as_str().to_owned(),
                id,
                source.event.as_str().to_owned(),
            )
        }
    };

    let mut postings = Vec::with_capacity(input.postings.len());
    let mut amount_sum: i64 = 0;
    let mut base_sum: i64 = 0;
    for (index, posting) in input.postings.iter().enumerate() {
        let amount = amount_cents(&format!("posting {index}'s amount"), posting.amount_cents)?;
        let base = amount_cents(
            &format!("posting {index}'s base-currency amount"),
            posting.base_cents,
        )?;
        // A posting that moves no money in either currency is a typo. Zero in
        // the document column alone is legitimate exactly once: the exchange
        // difference on a foreign-currency settlement, which moves the invoice's
        // dollars exactly and a different number of euro.
        if amount == 0 && base == 0 {
            return Err(StoreError::Validation(format!(
                "posting {index} moves no money in either currency"
            )));
        }
        if posting.account_id.as_str().trim().is_empty() {
            return Err(StoreError::Validation(format!(
                "posting {index} names no account"
            )));
        }
        amount_sum = accumulate(amount_sum, amount)?;
        base_sum = accumulate(base_sum, base)?;
        postings.push(NormalizedPosting {
            account_id: posting.account_id.as_str().to_owned(),
            amount_cents: amount,
            base_cents: base,
            vat_rate_bp: posting.vat_rate_bp.map(vat_rate_bp).transpose()?,
            customer_id: dimension("a posting's customer", posting.customer_id.as_ref())?,
            supplier_key: dimension("a posting's supplier", posting.supplier_key.as_ref())?,
            project_id: dimension("a posting's project", posting.project_id.as_ref())?,
            user_id: dimension("a posting's user", posting.user_id.as_ref())?,
            memo: bounded("a posting memo", &posting.memo, MEMO_MAX_CHARS)?,
        });
    }

    // **The invariant.** Both columns, because an entry that balances in euro
    // and not in dollars is two different stories about one event. The message
    // states the difference so a human can find the line, and names no stored
    // data (law 1).
    if amount_sum != 0 {
        return Err(StoreError::Validation(format!(
            "the entry does not balance: its debits and credits differ by \
             {amount_sum} cents in {currency}"
        )));
    }
    if base_sum != 0 {
        return Err(StoreError::Validation(format!(
            "the entry does not balance in the accounting currency: its debits and \
             credits differ by {base_sum} cents in {fx_base_currency}"
        )));
    }

    Ok(Normalized {
        entry_date: input.entry_date,
        kind: input.kind.as_str(),
        source_kind,
        source_id,
        source_event,
        memo: bounded("an entry memo", &input.memo, MEMO_MAX_CHARS)?,
        reverses_entry_id: input
            .reverses_entry_id
            .as_ref()
            .map(|id| id.as_str().to_owned()),
        attachment_node_id: dimension("an entry attachment", input.attachment_node_id.as_ref())?,
        currency,
        fx_base_currency,
        fx_rate_micro,
        fx_rate_date: input.fx.rate_date,
        postings,
    })
}

/// Turns the journal's uniqueness violation into the conflict that names what
/// actually happened, and leaves every other database failure alone.
///
/// The one that matters is the idempotency index: a document event that is
/// already posted is a `409`, not a second set of postings, and the caller
/// (which is usually a retry) can carry on.
fn map_journal_conflict(error: sqlx::Error) -> StoreError {
    match error {
        sqlx::Error::Database(ref db) if db.code().as_deref() == Some("23505") => {
            match db.constraint().unwrap_or_default() {
                "fin_entries_source_once" => {
                    StoreError::Conflict("this document event is already posted".to_owned())
                }
                _ => StoreError::Conflict("unique constraint".to_owned()),
            }
        }
        other => StoreError::Db(other),
    }
}

impl AccountStore {
    /// **The only way anything is ever written to the books.**
    ///
    /// Validates the entry whole (the balance rule, the shape of every
    /// posting, the account references), then writes the header and every
    /// posting in **one transaction**: a tenant is never left holding half an
    /// entry, and the balance check can never be true of what was checked and
    /// false of what was stored.
    ///
    /// Three refusals are worth knowing about before calling:
    ///
    /// - an entry whose postings do not add to zero — in either currency
    ///   column — is a [`StoreError::Validation`], and nothing is written;
    /// - a posting to an account that is not in this tenant's chart, or that
    ///   the tenant has **deactivated**, is refused naming the account: a
    ///   deactivated account is a tenant saying they are done with it, and
    ///   posting to it anyway would be us deciding they did not mean it;
    /// - a document event that is already posted is a [`StoreError::Conflict`]
    ///   — the idempotency the whole module is built on.
    ///
    /// # Errors
    /// [`StoreError::Validation`] for any of the above shape rules;
    /// [`StoreError::NotFound`] when the entry claims to reverse one that is
    /// not this tenant's; [`StoreError::Conflict`] when the document event is
    /// already posted; [`StoreError::Db`] on failure.
    pub async fn post_fin_entry(&self, input: &NewEntry) -> Result<FinEntryId> {
        let mut tx = self.pool.begin().await.map_err(StoreError::Db)?;
        let id = self.post_fin_entry_in(&mut tx, input).await?;
        tx.commit().await.map_err(StoreError::Db)?;
        Ok(id)
    }

    /// [`AccountStore::post_fin_entry`], inside a transaction the caller owns.
    ///
    /// The books and the thing that caused them to move belong in **one**
    /// transaction — a confirmed bank match writes a payment and posts its
    /// settlement, and a tenant must never be left holding one without the
    /// other ([`crate::bank_reconcile`]). Every rule and every refusal is the
    /// public door's; only the `BEGIN` and the `COMMIT` move to the caller.
    ///
    /// # Errors
    /// Exactly [`AccountStore::post_fin_entry`]'s. A caller must **not** catch
    /// them and carry on inside the same transaction: an error here has already
    /// poisoned it, and the only correct next step is to drop it.
    pub(crate) async fn post_fin_entry_in(
        &self,
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
        input: &NewEntry,
    ) -> Result<FinEntryId> {
        let entry = normalize(input)?;
        let id = FinEntryId::generate();

        // A reversal must correct one of THIS tenant's entries, and may not be
        // dated before it: a correction that predates what it corrects would
        // move money out of a period that was already reported.
        if let Some(target) = &entry.reverses_entry_id {
            let original: Option<Date> = sqlx::query_scalar(
                "SELECT entry_date FROM fin_entries WHERE tenant_id = $1 AND id = $2",
            )
            .bind(self.tenant.as_str())
            .bind(target)
            .fetch_optional(&mut **tx)
            .await
            .map_err(StoreError::Db)?;
            let original = original.ok_or(StoreError::NotFound)?;
            if entry.entry_date < original {
                return Err(StoreError::Validation(
                    "a reversal cannot be dated before the entry it corrects".to_owned(),
                ));
            }
        }

        // Every account named must be in this tenant's chart and active. The
        // foreign key alone would catch a foreign-tenant id, but as an opaque
        // 23503; asking first is what lets the caller be told which account and
        // why — and it is where the chart's `active` flag becomes a rule about
        // new postings rather than only about pickers.
        let mut wanted: Vec<String> = entry
            .postings
            .iter()
            .map(|posting| posting.account_id.clone())
            .collect();
        wanted.sort_unstable();
        wanted.dedup();
        let known = sqlx::query_as::<_, (String, String, bool)>(
            "SELECT id, code, active FROM fin_accounts WHERE tenant_id = $1 AND id = ANY($2)",
        )
        .bind(self.tenant.as_str())
        .bind(wanted.as_slice())
        .fetch_all(&mut **tx)
        .await
        .map_err(StoreError::Db)?;
        for id in &wanted {
            match known.iter().find(|(known_id, _, _)| known_id == id) {
                None => {
                    return Err(StoreError::Validation(
                        "the entry posts to an account that is not in this chart".to_owned(),
                    ));
                }
                Some((_, code, false)) => {
                    return Err(StoreError::Validation(format!(
                        "the entry posts to account {code}, which is deactivated"
                    )));
                }
                Some(_) => {}
            }
        }

        sqlx::query(
            "INSERT INTO fin_entries (tenant_id, id, entry_date, kind, source_kind, source_id, \
                 source_event, memo, reverses_entry_id, attachment_node_id, currency, \
                 fx_base_currency, fx_rate_micro, fx_rate_date, created_by) \
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15)",
        )
        .bind(self.tenant.as_str())
        .bind(id.as_str())
        .bind(entry.entry_date)
        .bind(entry.kind)
        .bind(&entry.source_kind)
        .bind(&entry.source_id)
        .bind(&entry.source_event)
        .bind(&entry.memo)
        .bind(entry.reverses_entry_id.as_deref())
        .bind(entry.attachment_node_id.as_deref())
        .bind(&entry.currency)
        .bind(&entry.fx_base_currency)
        .bind(entry.fx_rate_micro)
        .bind(entry.fx_rate_date)
        .bind(self.user.as_str())
        .execute(&mut **tx)
        .await
        .map_err(map_journal_conflict)?;

        for (position, posting) in entry.postings.iter().enumerate() {
            sqlx::query(
                "INSERT INTO fin_postings (tenant_id, id, entry_id, position, account_id, \
                     amount_cents, base_cents, vat_rate_bp, customer_id, supplier_key, \
                     project_id, user_id, memo) \
                 VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13)",
            )
            .bind(self.tenant.as_str())
            .bind(FinPostingId::generate().as_str())
            .bind(id.as_str())
            .bind(i32::try_from(position).unwrap_or(i32::MAX))
            .bind(&posting.account_id)
            .bind(posting.amount_cents)
            .bind(posting.base_cents)
            .bind(posting.vat_rate_bp)
            .bind(posting.customer_id.as_deref())
            .bind(posting.supplier_key.as_deref())
            .bind(posting.project_id.as_deref())
            .bind(posting.user_id.as_deref())
            .bind(&posting.memo)
            .execute(&mut **tx)
            .await
            .map_err(map_journal_conflict)?;
        }

        Ok(id)
    }

    /// One entry header of the tenant, or `None` — including when the id
    /// belongs to another tenant (indistinguishable by design).
    ///
    /// # Errors
    /// [`StoreError::Db`] on failure; [`StoreError::Validation`] when a stored
    /// word is one this build does not know (a schema disagreement, which is
    /// honest to fail on rather than to guess through).
    pub async fn fin_entry(&self, id: &FinEntryId) -> Result<Option<Entry>> {
        let row = sqlx::query_as::<_, EntryRow>(&format!(
            "SELECT {ENTRY_COLS} FROM fin_entries WHERE tenant_id = $1 AND id = $2"
        ))
        .bind(self.tenant.as_str())
        .bind(id.as_str())
        .fetch_optional(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        row.map(EntryRow::into_entry).transpose()
    }

    /// Every posting of one entry, in the order it was written. Empty for an
    /// entry that is not this tenant's — a posting is never readable without
    /// its header.
    ///
    /// # Errors
    /// [`StoreError::Db`] on failure.
    pub async fn fin_entry_postings(&self, id: &FinEntryId) -> Result<Vec<Posting>> {
        let rows = sqlx::query_as::<_, PostingRow>(&format!(
            "SELECT {POSTING_COLS} FROM fin_postings \
             WHERE tenant_id = $1 AND entry_id = $2 ORDER BY position"
        ))
        .bind(self.tenant.as_str())
        .bind(id.as_str())
        .fetch_all(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        Ok(rows.into_iter().map(PostingRow::into_posting).collect())
    }

    /// An entry with its postings — the journal read a screen, a CSV and a
    /// reversal rule all make.
    ///
    /// # Errors
    /// [`StoreError::Db`] on failure; [`StoreError::Validation`] on a stored
    /// word this build does not know.
    pub async fn fin_journal_entry(&self, id: &FinEntryId) -> Result<Option<JournalEntry>> {
        let Some(entry) = self.fin_entry(id).await? else {
            return Ok(None);
        };
        let postings = self.fin_entry_postings(id).await?;
        Ok(Some(JournalEntry { entry, postings }))
    }

    /// The tenant's journal over a date range, newest accounting date first,
    /// capped at [`JOURNAL_PAGE_MAX`].
    ///
    /// Both bounds are inclusive and either may be absent; `limit` is clamped
    /// rather than refused, because a caller asking for everything wants as
    /// much as we will give rather than an error.
    ///
    /// # Errors
    /// [`StoreError::Db`] on failure; [`StoreError::Validation`] on a stored
    /// word this build does not know.
    pub async fn fin_entries(
        &self,
        from: Option<Date>,
        to: Option<Date>,
        limit: i64,
    ) -> Result<Vec<Entry>> {
        let rows = sqlx::query_as::<_, EntryRow>(&format!(
            "SELECT {ENTRY_COLS} FROM fin_entries \
             WHERE tenant_id = $1 AND ($2::date IS NULL OR entry_date >= $2) \
                 AND ($3::date IS NULL OR entry_date <= $3) \
             ORDER BY entry_date DESC, created_at DESC, id DESC LIMIT $4"
        ))
        .bind(self.tenant.as_str())
        .bind(from)
        .bind(to)
        .bind(limit.clamp(1, JOURNAL_PAGE_MAX))
        .fetch_all(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        rows.into_iter().map(EntryRow::into_entry).collect()
    }

    /// The entry a document event already produced, if it has been posted.
    ///
    /// This is the *question* behind the idempotency key, and a caller that
    /// would rather look than catch a conflict (a backfill, a screen showing
    /// "booked") asks it here.
    ///
    /// # Errors
    /// [`StoreError::Db`] on failure.
    pub async fn fin_entry_for_source(&self, source: &EntrySource) -> Result<Option<FinEntryId>> {
        self.fin_entry_for_source_on(&self.pool, source).await
    }

    /// [`AccountStore::fin_entry_for_source`] against any executor, so the same
    /// question can be asked **inside** a transaction that is about to post.
    ///
    /// A caller in a transaction has to *look* rather than catch: an already
    /// posted document raises a conflict from the unique index, and a conflict
    /// inside a transaction has already aborted it.
    ///
    /// # Errors
    /// [`StoreError::Db`] on failure.
    pub(crate) async fn fin_entry_for_source_on<'e, E>(
        &self,
        executor: E,
        source: &EntrySource,
    ) -> Result<Option<FinEntryId>>
    where
        E: sqlx::Executor<'e, Database = sqlx::Postgres>,
    {
        let row: Option<String> = sqlx::query_scalar(
            "SELECT id FROM fin_entries WHERE tenant_id = $1 AND source_kind = $2 \
                 AND source_id = $3 AND source_event = $4",
        )
        .bind(self.tenant.as_str())
        .bind(source.kind.as_str())
        .bind(source.id.trim())
        .bind(source.event.as_str())
        .fetch_optional(executor)
        .await
        .map_err(StoreError::Db)?;
        Ok(row.map(FinEntryId::new))
    }

    /// **The health query**: every entry of this tenant whose postings do not
    /// add to zero. Always empty, and the test suite asserts it after every
    /// property run (B4.03b) rather than trusting the in-memory check that
    /// wrote them.
    ///
    /// An entry with no postings at all counts as unbalanced: the write path
    /// cannot produce one, so if it exists something else wrote it.
    ///
    /// # Errors
    /// [`StoreError::Db`] on failure.
    pub async fn fin_unbalanced_entries(&self) -> Result<Vec<FinEntryId>> {
        let rows: Vec<String> = sqlx::query_scalar(
            "SELECT e.id FROM fin_entries e \
             LEFT JOIN fin_postings p ON p.tenant_id = e.tenant_id AND p.entry_id = e.id \
             WHERE e.tenant_id = $1 \
             GROUP BY e.id \
             HAVING COALESCE(SUM(p.amount_cents), 0) <> 0 \
                 OR COALESCE(SUM(p.base_cents), 0) <> 0 \
                 OR COUNT(p.id) = 0 \
             ORDER BY e.id",
        )
        .bind(self.tenant.as_str())
        .fetch_all(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        Ok(rows.into_iter().map(FinEntryId::new).collect())
    }
}

// ---- row types --------------------------------------------------------------

#[derive(sqlx::FromRow)]
struct EntryRow {
    id: String,
    entry_date: Date,
    kind: String,
    source_kind: String,
    source_id: String,
    source_event: String,
    memo: String,
    reverses_entry_id: Option<String>,
    attachment_node_id: Option<String>,
    currency: String,
    fx_base_currency: String,
    fx_rate_micro: i64,
    fx_rate_date: Date,
    created_by: String,
    created_at: OffsetDateTime,
}

impl EntryRow {
    /// Reads a row back into the typed record. The three enums are re-parsed
    /// rather than trusted: a word this build does not know is a schema
    /// disagreement, and answering with an error is honest where inventing a
    /// variant would be a wrong number on a report.
    fn into_entry(self) -> Result<Entry> {
        let source = if self.source_kind.is_empty() {
            None
        } else {
            Some(EntrySource {
                kind: SourceKind::parse(&self.source_kind)?,
                id: self.source_id,
                event: SourceEvent::parse(&self.source_event)?,
            })
        };
        Ok(Entry {
            id: FinEntryId::new(self.id),
            entry_date: self.entry_date,
            kind: EntryKind::parse(&self.kind)?,
            source,
            memo: self.memo,
            reverses_entry_id: self.reverses_entry_id.map(FinEntryId::new),
            attachment_node_id: self.attachment_node_id,
            currency: self.currency,
            fx: FxSnapshot {
                base_currency: self.fx_base_currency,
                rate_micro: self.fx_rate_micro,
                rate_date: self.fx_rate_date,
            },
            created_by: self.created_by,
            created_at: self.created_at,
        })
    }
}

#[derive(sqlx::FromRow)]
struct PostingRow {
    id: String,
    entry_id: String,
    position: i32,
    account_id: String,
    amount_cents: i64,
    base_cents: i64,
    vat_rate_bp: Option<i32>,
    customer_id: Option<String>,
    supplier_key: Option<String>,
    project_id: Option<String>,
    user_id: Option<String>,
    memo: String,
}

impl PostingRow {
    fn into_posting(self) -> Posting {
        Posting {
            id: FinPostingId::new(self.id),
            entry_id: FinEntryId::new(self.entry_id),
            position: self.position,
            account_id: FinAccountId::new(self.account_id),
            amount_cents: self.amount_cents,
            base_cents: self.base_cents,
            vat_rate_bp: self.vat_rate_bp,
            customer_id: self.customer_id,
            supplier_key: self.supplier_key,
            project_id: self.project_id,
            user_id: self.user_id,
            memo: self.memo,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    fn day(day: u8) -> Date {
        Date::from_calendar_date(2026, time::Month::March, day).unwrap_or(Date::MIN)
    }

    fn account(tag: &str) -> FinAccountId {
        FinAccountId::new(tag)
    }

    /// The smallest true entry: €100 of revenue invoiced, in the currency the
    /// books are kept in.
    fn invoice_entry() -> NewEntry {
        NewEntry {
            entry_date: day(4),
            kind: EntryKind::Invoice,
            source: Some(EntrySource {
                kind: SourceKind::Invoice,
                id: "inv-1".to_owned(),
                event: SourceEvent::Issue,
            }),
            memo: "INV-2026-00001".to_owned(),
            reverses_entry_id: None,
            attachment_node_id: None,
            currency: "EUR".to_owned(),
            fx: FxSnapshot::identity("EUR", day(4)),
            postings: vec![
                NewPosting::new(account("ar"), 12_100, 12_100),
                NewPosting::new(account("revenue"), -10_000, -10_000),
                NewPosting {
                    vat_rate_bp: Some(2100),
                    ..NewPosting::new(account("vat"), -2_100, -2_100)
                },
            ],
        }
    }

    fn invalid<T: std::fmt::Debug>(result: Result<T>) -> String {
        match result {
            Err(StoreError::Validation(msg)) => msg,
            other => panic!("expected Validation, got {other:?}"),
        }
    }

    #[test]
    fn a_balanced_entry_normalises_whole() {
        let e = normalize(&invoice_entry()).unwrap_or_else(|err| panic!("rejected: {err}"));
        assert_eq!(e.kind, "invoice");
        assert_eq!(e.source_kind, "invoice");
        assert_eq!(e.source_id, "inv-1");
        assert_eq!(e.source_event, "issue");
        assert_eq!(e.currency, "EUR");
        assert_eq!(e.fx_rate_micro, IDENTITY_RATE_MICRO);
        assert_eq!(e.postings.len(), 3);
        assert_eq!(e.postings[2].vat_rate_bp, Some(2100));
        // The order the rule wrote is the order the entry keeps: `position` is
        // the index, so the read gives back what was posted.
        assert_eq!(e.postings[0].amount_cents, 12_100);
    }

    #[test]
    fn an_unbalanced_entry_is_refused_in_both_columns() {
        // The document column: a VAT posting a cent short.
        let mut short = invoice_entry();
        short.postings[2].amount_cents = -2_099;
        short.postings[2].base_cents = -2_099;
        let msg = invalid(normalize(&short));
        assert!(msg.contains("does not balance"), "{msg}");
        assert!(
            msg.contains('1'),
            "the message states the difference: {msg}"
        );

        // The base column alone: the document balances, the euro do not. This
        // is the one an eyeball misses and a report finds at the year end.
        let mut base_only = invoice_entry();
        base_only.currency = "USD".to_owned();
        base_only.fx = FxSnapshot {
            base_currency: "EUR".to_owned(),
            rate_micro: 1_100_000,
            rate_date: day(4),
        };
        base_only.postings[0].base_cents = 11_001;
        base_only.postings[1].base_cents = -9_091;
        base_only.postings[2].base_cents = -1_909;
        let msg = invalid(normalize(&base_only));
        assert!(msg.contains("accounting currency"), "{msg}");
        assert!(msg.contains("EUR"), "{msg}");
    }

    #[test]
    fn an_entry_needs_at_least_two_postings() {
        let mut single = invoice_entry();
        single.postings.truncate(1);
        assert!(invalid(normalize(&single)).contains("two postings"));

        let mut none = invoice_entry();
        none.postings.clear();
        assert!(
            invalid(normalize(&none)).contains("two postings"),
            "an empty entry sums to zero and is still not an entry"
        );

        let mut many = invoice_entry();
        many.postings = (0..=ENTRY_POSTINGS_MAX)
            .map(|_| NewPosting::new(account("ar"), 0, 1))
            .collect();
        assert!(invalid(normalize(&many)).contains("at most"));
    }

    #[test]
    fn a_posting_that_moves_no_money_is_refused() {
        let mut still = invoice_entry();
        still.postings.push(NewPosting::new(account("bank"), 0, 0));
        assert!(invalid(normalize(&still)).contains("moves no money"));

        // Zero in the document column alone is the exchange difference, and it
        // is legitimate: the settlement moves the invoice's dollars exactly and
        // a different number of euro.
        let mut fx = invoice_entry();
        fx.currency = "USD".to_owned();
        fx.fx = FxSnapshot {
            base_currency: "EUR".to_owned(),
            rate_micro: 1_100_000,
            rate_date: day(4),
        };
        fx.postings[0].base_cents = 11_001;
        fx.postings[1].base_cents = -9_091;
        fx.postings[2].base_cents = -1_909;
        fx.postings.push(NewPosting::new(account("fx_diff"), 0, -1));
        assert!(
            normalize(&fx).is_ok(),
            "the exchange difference is a real posting"
        );
    }

    #[test]
    fn amounts_are_bounded_and_never_wrap() {
        let mut big = invoice_entry();
        big.postings[0].amount_cents = POSTING_AMOUNT_MAX_CENTS + 1;
        assert!(invalid(normalize(&big)).contains("cents"));

        let mut small = invoice_entry();
        small.postings[0].amount_cents = -POSTING_AMOUNT_MAX_CENTS - 1;
        assert!(invalid(normalize(&small)).contains("cents"));

        // The ceiling is on the magnitude, not the sign: a credit of the
        // largest amount is as legitimate as a debit of it.
        let mut edge = invoice_entry();
        edge.postings = vec![
            NewPosting::new(account("ar"), POSTING_AMOUNT_MAX_CENTS, 1),
            NewPosting::new(account("revenue"), -POSTING_AMOUNT_MAX_CENTS, -1),
        ];
        assert!(normalize(&edge).is_ok());
    }

    #[test]
    fn the_identity_rate_is_required_in_the_accounting_currency() {
        let mut wrong = invoice_entry();
        wrong.fx = FxSnapshot {
            base_currency: "EUR".to_owned(),
            rate_micro: 1_100_000,
            rate_date: day(4),
        };
        assert!(invalid(normalize(&wrong)).contains("identity"));

        let mut foreign = invoice_entry();
        foreign.currency = "usd".to_owned();
        foreign.fx = FxSnapshot {
            base_currency: "eur".to_owned(),
            rate_micro: 1_100_000,
            rate_date: day(3),
        };
        foreign.postings[0].base_cents = 11_000;
        foreign.postings[1].base_cents = -9_091;
        foreign.postings[2].base_cents = -1_909;
        let e = normalize(&foreign).unwrap_or_else(|err| panic!("rejected: {err}"));
        assert_eq!(e.currency, "USD", "currency codes are stored uppercased");
        assert_eq!(e.fx_base_currency, "EUR");
    }

    #[test]
    fn a_rate_must_be_a_usable_number() {
        let mut zero = invoice_entry();
        zero.fx = FxSnapshot {
            base_currency: "USD".to_owned(),
            rate_micro: 0,
            rate_date: day(4),
        };
        assert!(invalid(normalize(&zero)).contains("exchange rate"));
    }

    #[test]
    fn a_source_is_all_three_parts_or_none() {
        let mut blank = invoice_entry();
        blank.source = Some(EntrySource {
            kind: SourceKind::Invoice,
            id: "   ".to_owned(),
            event: SourceEvent::Issue,
        });
        assert!(invalid(normalize(&blank)).contains("source document id"));

        let mut manual = invoice_entry();
        manual.kind = EntryKind::Manual;
        manual.source = None;
        let e = normalize(&manual).unwrap_or_else(|err| panic!("rejected: {err}"));
        assert_eq!(e.source_kind, "");
        assert_eq!(e.source_id, "");
        assert_eq!(e.source_event, "", "a manual entry answers to nothing");
    }

    #[test]
    fn dimensions_and_memos_are_trimmed_and_bounded() {
        let mut e = invoice_entry();
        e.memo = format!("  {}  ", "m".repeat(4));
        e.postings[0].customer_id = Some("  cust-1  ".to_owned());
        e.postings[0].project_id = Some("   ".to_owned());
        e.postings[0].memo = "  a line  ".to_owned();
        let n = normalize(&e).unwrap_or_else(|err| panic!("rejected: {err}"));
        assert_eq!(n.memo, "mmmm");
        assert_eq!(n.postings[0].customer_id.as_deref(), Some("cust-1"));
        assert_eq!(
            n.postings[0].project_id, None,
            "a blank dimension is absent, not an empty group"
        );
        assert_eq!(n.postings[0].memo, "a line");

        let mut long = invoice_entry();
        long.memo = "m".repeat(MEMO_MAX_CHARS + 1);
        assert!(invalid(normalize(&long)).contains("at most"));

        let mut wide = invoice_entry();
        wide.postings[0].customer_id = Some("c".repeat(DIMENSION_MAX_CHARS + 1));
        assert!(invalid(normalize(&wide)).contains("at most"));
    }

    #[test]
    fn a_vat_rate_on_a_posting_is_the_billing_rule() {
        let mut e = invoice_entry();
        e.postings[2].vat_rate_bp = Some(10_001);
        assert!(invalid(normalize(&e)).contains("VAT rate"));

        let mut zero = invoice_entry();
        zero.postings[2].vat_rate_bp = Some(0);
        assert!(
            normalize(&zero).is_ok(),
            "0 % is exempt, reverse-charge and intra-Community — all real"
        );
    }

    #[test]
    fn an_account_is_required_on_every_posting() {
        let mut e = invoice_entry();
        e.postings[1].account_id = FinAccountId::new("  ");
        assert!(invalid(normalize(&e)).contains("names no account"));
    }

    #[test]
    fn entry_kinds_round_trip_and_reject_invention() {
        let mut seen = HashSet::new();
        for kind in EntryKind::ALL {
            assert!(seen.insert(kind.as_str()), "duplicate word");
            assert_eq!(
                EntryKind::parse(kind.as_str()).unwrap_or_else(|e| panic!("rejected: {e}")),
                *kind
            );
            // The migration's CHECK accepts `^[a-z][a-z_]{0,30}$`.
            assert!(kind.as_str().starts_with(|c: char| c.is_ascii_lowercase()));
            assert!(
                kind.as_str()
                    .chars()
                    .all(|c| c.is_ascii_lowercase() || c == '_')
            );
            assert!(kind.as_str().chars().count() <= 31);
        }
        for bad in ["", "Invoice", "sale", "journal"] {
            assert!(invalid(EntryKind::parse(bad)).contains("entry kind"));
        }
    }

    #[test]
    fn source_words_round_trip_and_reject_invention() {
        for kind in SourceKind::ALL {
            assert_eq!(
                SourceKind::parse(kind.as_str()).unwrap_or_else(|e| panic!("rejected: {e}")),
                *kind
            );
            assert!(
                kind.as_str()
                    .chars()
                    .all(|c| c.is_ascii_lowercase() || c == '_')
            );
        }
        for event in SourceEvent::ALL {
            assert_eq!(
                SourceEvent::parse(event.as_str()).unwrap_or_else(|e| panic!("rejected: {e}")),
                *event
            );
        }
        // An invented word would let one document post twice under two
        // spellings, which is exactly what the idempotency key exists to stop.
        for bad in ["", "quote", "Invoice"] {
            assert!(invalid(SourceKind::parse(bad)).contains("source kind"));
        }
        for bad in ["", "paid", "Issue"] {
            assert!(invalid(SourceEvent::parse(bad)).contains("source event"));
        }
    }

    #[test]
    fn the_debit_and_credit_columns_are_the_sign_read_out() {
        let posting = Posting {
            id: FinPostingId::new("p"),
            entry_id: FinEntryId::new("e"),
            position: 0,
            account_id: account("ar"),
            amount_cents: 12_100,
            base_cents: 12_100,
            vat_rate_bp: None,
            customer_id: None,
            supplier_key: None,
            project_id: None,
            user_id: None,
            memo: String::new(),
        };
        assert_eq!(posting.debit_cents(), 12_100);
        assert_eq!(posting.credit_cents(), 0);
        let credit = Posting {
            amount_cents: -2_100,
            ..posting
        };
        assert_eq!(credit.debit_cents(), 0);
        assert_eq!(credit.credit_cents(), 2_100);
    }

    #[test]
    fn adding_up_refuses_to_wrap() {
        assert!(accumulate(i64::MAX, 1).is_err());
        assert!(accumulate(i64::MIN, -1).is_err());
        assert_eq!(accumulate(5, -5).unwrap_or(1), 0);
    }
}
