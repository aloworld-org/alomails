//! The bank statement, and the lines it stages (alo Finance, ADR 0035, wave
//! B4.08; `docs/design/finance.md`, "The bank and reconciliation").
//!
//! # A staged line is not an event
//!
//! Everything else alo books is posted the moment it becomes real: an invoice
//! issues and the journal moves, a payment lands and the journal moves. A bank
//! line is deliberately the opposite. It is **what the bank says happened**,
//! held apart from the books until a person says what it *was* — which invoice
//! it paid, which expense it settled, or that it is not ours to book at all.
//!
//! So nothing in this module posts, and nothing in it matches. Confirming a
//! match is B4.09's verb, and confirming is what creates the payment and its
//! postings. ADR 0023's propose-then-approve rule is a money rule here: a wrong
//! automatic match marks an invoice paid that is not, and the customer stops
//! being chased.
//!
//! # Three parsers, one contract
//!
//! [`ParsedStatement`] is the shape all three bank formats read into —
//! CAMT.053 ([`crate::bank_camt`], B4.08a), MT940 (B4.08b) and mapped CSV
//! (B4.08c). Everything below the parser is written once: the same validation,
//! the same duplicate rules, the same import report. The parsers disagree only
//! about what they can *tell* us — an MT940 line names no IBAN, a CSV names
//! whatever the mapping said — which is why [`BankStatement::source`] is stored:
//! a reader of a blank field needs to know which silence it is looking at.
//!
//! # A file imports once and a line imports once
//!
//! Bookkeepers re-upload. Banks publish overlapping files, and a month's
//! statement arrives again inside the quarter's. Two rules carry the whole of
//! that story, and both are **per tenant**:
//!
//! - The file's SHA-256. The same bytes are the same import, refused as a
//!   [`StoreError::Conflict`] naming the period already held — swallowing it
//!   would leave a bookkeeper looking for lines in the wrong place.
//! - The line's hash ([`line_hash`]). A line already staged from another file is
//!   skipped, and [`BankImport`] says how many were and why.
//!
//! *Rejected: trusting the bank's own reference alone.* Some banks reuse a
//! reference across statements and some omit it entirely, and neither failure
//! is visible until money is booked twice.
//!
//! # The door
//!
//! A bank statement is the company's, not one employee's: every statement binds
//! `tenant_id` and nothing binds `user_id`, so a colleague sees the same
//! imports. Who may *upload* one is an edge decision (B4.08c). What never
//! happens on any door is one tenant reading another's: the file hash and the
//! line hash are unique **per tenant**, so two tenants banking at the same
//! institution can hold byte-identical files without either becoming an oracle
//! for the other.

use std::collections::HashMap;

use sha2::{Digest, Sha256};
use time::{Date, OffsetDateTime};

use crate::account::AccountStore;
use crate::billing_field::currency as currency_code;
use crate::error::{Result, StoreError};
use crate::id::{BankLineId, BankStatementId, UserId};

/// The largest statement file we will read. Big enough for a year of a busy
/// account in CAMT (the wordiest of the three formats), small enough that one
/// upload cannot hold a worker.
pub const MAX_BANK_FILE_BYTES: usize = 8 * 1024 * 1024;

/// The most transactions one file may state. A month of a busy account is a few
/// hundred; the cap refuses a file that is trying to be a denial of service,
/// naming itself rather than failing somewhere deep in a loop.
pub const STATEMENT_LINES_MAX: usize = 5_000;

/// Longest counterparty name we keep — ISO 20022's own `Max140Text`.
pub const COUNTERPARTY_NAME_MAX: usize = 140;

/// Longest remittance we keep. A structured CAMT entry can state several
/// unstructured lines and a creditor reference; joined, they are what B4.09
/// searches our invoice numbers in.
pub const REMITTANCE_MAX: usize = 1_000;

/// Longest bank reference we keep. ISO 20022 says `Max35Text`; the slack is
/// deliberate, because a bank that writes 40 characters into it has still told
/// us something useful.
pub const BANK_REF_MAX: usize = 140;

/// Longest statement identifier we keep (`<Stmt><Id>`, `:28C:`).
pub const STATEMENT_REF_MAX: usize = 140;

/// The typo guard on a single transaction: ±10 billion cents. The same ceiling
/// every other alo money column carries.
pub const LINE_AMOUNT_MAX_CENTS: i64 = 1_000_000_000_000;

/// Most lines one read returns — deliberately the same figure as
/// [`STATEMENT_LINES_MAX`], so that a read narrowed to **one import** can never
/// come back short. That is the read the reconciliation screen makes, and a
/// bookkeeper working a silently truncated statement would reconcile a month
/// that is missing transactions.
///
/// An un-narrowed read of a tenant's whole history *can* reach the cap once
/// they have imported enough months, which is what the `statement` and `status`
/// filters are for until B4.08c's route offers real paging.
pub const BANK_LINES_PAGE_MAX: i64 = STATEMENT_LINES_MAX as i64;

/// The columns every read of a statement selects, in [`StatementRow`] order.
const STATEMENT_COLS: &str = "id, account_iban, currency, source, statement_ref, file_sha256, \
     opening_balance_cents, closing_balance_cents, from_date, to_date, imported_by, imported_at, \
     (SELECT count(*) FROM bank_lines l \
      WHERE l.tenant_id = s.tenant_id AND l.statement_id = s.id) AS line_count";

/// The columns every read of a line selects, in [`LineRow`] order.
const LINE_COLS: &str = "id, statement_id, line_no, booked_on, value_on, amount_cents, currency, \
     counterparty_name, counterparty_iban, remittance, bank_ref, status, ignored_reason, \
     created_at";

/// Which parser read the file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BankSource {
    /// ISO 20022 CAMT.053 — the XML end-of-day statement (B4.08a).
    Camt,
    /// SWIFT MT940 — the line format that predates it and outlives it (B4.08b).
    Mt940,
    /// A CSV export plus the mapping a person confirmed (B4.08c).
    Csv,
}

impl BankSource {
    /// The value this source is stored and reported as.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Camt => "camt",
            Self::Mt940 => "mt940",
            Self::Csv => "csv",
        }
    }

    /// The source a stored value names, or `None` when it is not one of ours.
    #[must_use]
    pub fn parse(raw: &str) -> Option<Self> {
        match raw {
            "camt" => Some(Self::Camt),
            "mt940" => Some(Self::Mt940),
            "csv" => Some(Self::Csv),
            _ => None,
        }
    }
}

/// Where a staged line stands.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BankLineStatus {
    /// Nobody has said what this line was. Every line starts here.
    Unmatched,
    /// A confirmed match exists (B4.09), and it is what created the payment.
    Matched,
    /// A person has said this line is not ours to book — bank charges the
    /// tenant handles elsewhere, a transfer between their own accounts.
    Ignored,
}

impl BankLineStatus {
    /// The value this status is stored and reported as.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Unmatched => "unmatched",
            Self::Matched => "matched",
            Self::Ignored => "ignored",
        }
    }

    /// The status a stored value names, or `None` when it is not one of ours.
    #[must_use]
    pub fn parse(raw: &str) -> Option<Self> {
        match raw {
            "unmatched" => Some(Self::Unmatched),
            "matched" => Some(Self::Matched),
            "ignored" => Some(Self::Ignored),
            _ => None,
        }
    }
}

/// A statement as a parser read it, before anything has been validated or
/// stored. The one shape all three formats produce.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedStatement {
    /// Which parser produced this.
    pub source: BankSource,
    /// The account the statement is of, as the file states it.
    pub account_iban: String,
    /// The statement's currency, ISO 4217.
    pub currency: String,
    /// The bank's own name for this statement, or `""`.
    pub statement_ref: String,
    /// What the bank said the account held at the start of the period. `None`
    /// when the file states no such balance — absent, not zero.
    pub opening_balance_cents: Option<i64>,
    /// What the bank said it held at the end. `None` when unstated.
    pub closing_balance_cents: Option<i64>,
    /// The first day the statement covers.
    pub from_date: Date,
    /// The last day it covers.
    pub to_date: Date,
    /// The transactions, in the order the file listed them.
    pub lines: Vec<ParsedLine>,
    /// How many entries the file stated that are **not booked yet** and were
    /// therefore not staged. A CAMT.053 is an end-of-day statement and should
    /// carry none; some banks put pending items in one anyway, and a pending
    /// item is not something to reconcile against.
    pub unbooked: usize,
}

/// One transaction as a parser read it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedLine {
    /// The day the bank posted it — the day the books use.
    pub booked_on: Date,
    /// The day interest counts from, which is sometimes earlier and is often
    /// the day the customer believes they paid.
    pub value_on: Date,
    /// Signed integer cents: positive is money in, negative is money out. The
    /// wire formats say it another way; normalising the sign at the parser is
    /// what stops every reader after it re-deciding which way a number points.
    pub amount_cents: i64,
    /// This line's own currency, which is usually but not always the
    /// statement's.
    pub currency: String,
    /// Who the money came from (on a credit) or went to (on a debit).
    pub counterparty_name: String,
    /// Their IBAN, when the file states one.
    pub counterparty_iban: String,
    /// What the payer wrote on it.
    pub remittance: String,
    /// The bank's own reference for the entry.
    pub bank_ref: String,
}

/// A stored statement.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BankStatement {
    /// Our id for the import.
    pub id: BankStatementId,
    /// The account it is of.
    pub account_iban: String,
    /// Its currency.
    pub currency: String,
    /// Which parser read it.
    pub source: BankSource,
    /// The bank's own name for it, or `""`.
    pub statement_ref: String,
    /// SHA-256 of the file exactly as uploaded, lowercase hex.
    pub file_sha256: String,
    /// The stated opening balance, or `None`.
    pub opening_balance_cents: Option<i64>,
    /// The stated closing balance, or `None`.
    pub closing_balance_cents: Option<i64>,
    /// The first day covered.
    pub from_date: Date,
    /// The last day covered.
    pub to_date: Date,
    /// Who uploaded it.
    pub imported_by: UserId,
    /// When.
    pub imported_at: OffsetDateTime,
    /// How many lines are staged against it — after de-duplication, so an
    /// overlapping file honestly shows fewer lines than the file had entries.
    pub line_count: i64,
}

/// A stored, staged line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BankLine {
    /// Our id for the line.
    pub id: BankLineId,
    /// The import it came from.
    pub statement_id: BankStatementId,
    /// Where in that file it was, from 1.
    pub line_no: i32,
    /// The day the bank posted it.
    pub booked_on: Date,
    /// The day it takes value from.
    pub value_on: Date,
    /// Signed integer cents; positive is money in.
    pub amount_cents: i64,
    /// The line's currency.
    pub currency: String,
    /// Who the other party was, as the bank named them.
    pub counterparty_name: String,
    /// Their IBAN, or `""`.
    pub counterparty_iban: String,
    /// What was written on it.
    pub remittance: String,
    /// The bank's reference, or `""`.
    pub bank_ref: String,
    /// Where the line stands.
    pub status: BankLineStatus,
    /// Why it is not ours to book, when a person has said so
    /// ([`crate::bank_ignore`]); `""` for every other state. The sentence
    /// belongs on the line rather than only in the audit log, because it is
    /// what the next person to read the statement needs.
    pub ignored_reason: String,
    /// When it was staged.
    pub created_at: OffsetDateTime,
}

/// What one import did — the report a person reads after uploading a file.
///
/// The two counts are the whole reason it exists. "17 lines staged" alone
/// invites the question the second count answers: a file that overlaps last
/// month's is *supposed* to add fewer lines than it has entries, and a person
/// who is not told that will go looking for the missing ones.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BankImport {
    /// The statement as stored.
    pub statement: BankStatement,
    /// How many lines this file added.
    pub staged: usize,
    /// How many of its entries were already staged from another file.
    pub duplicates: usize,
    /// How many entries the file stated as not yet booked, and which were
    /// therefore not staged.
    pub unbooked: usize,
}

impl AccountStore {
    /// Imports an ISO 20022 CAMT.053 statement file.
    ///
    /// The single door for a CAMT file: parsing, validation, the duplicate
    /// rules and the write are one call, so no caller can perform half of them.
    ///
    /// # Errors
    /// [`StoreError::Validation`] when the file is not a readable CAMT.053 or
    /// states something we cannot hold exactly — the message names the entry
    /// and the field and **never quotes the file**, which is the tenant's bank
    /// data (Law 1); [`StoreError::Conflict`] when these exact bytes have
    /// already been imported; [`StoreError::Db`] on failure.
    pub async fn import_bank_camt053(&self, file: &[u8]) -> Result<BankImport> {
        let parsed = crate::bank_camt::parse_camt053(file)?;
        self.stage_bank_statement(&parsed, &sha256_hex(file)).await
    }

    /// Imports a SWIFT MT940 statement file.
    ///
    /// The same door as [`Self::import_bank_camt053`], one format along: the
    /// parser differs and nothing after it does, which is the whole point of
    /// [`ParsedStatement`]. Two files of the same month in the two formats
    /// therefore de-duplicate against each other line by line, because the hash
    /// is of what the bank said happened and not of how it spelled it.
    ///
    /// # Errors
    /// [`StoreError::Validation`] when the file is not a readable MT940 or
    /// states something we cannot hold exactly — the message names the
    /// transaction and the field and **never quotes the file**, which is the
    /// tenant's bank data (Law 1); [`StoreError::Conflict`] when these exact
    /// bytes have already been imported; [`StoreError::Db`] on failure.
    pub async fn import_bank_mt940(&self, file: &[u8]) -> Result<BankImport> {
        let parsed = crate::bank_mt940::parse_mt940(file)?;
        self.stage_bank_statement(&parsed, &sha256_hex(file)).await
    }

    /// Validates a parsed statement and stages it.
    ///
    /// Separate from the parsers so that all three formats — and the CSV
    /// wizard, which builds a [`ParsedStatement`] from a mapping rather than
    /// from a syntax — land through exactly the same rules.
    ///
    /// # Errors
    /// [`StoreError::Validation`] when a field breaks its rule;
    /// [`StoreError::Conflict`] on a file already imported; [`StoreError::Db`]
    /// on failure.
    pub(crate) async fn stage_bank_statement(
        &self,
        parsed: &ParsedStatement,
        file_sha256: &str,
    ) -> Result<BankImport> {
        let statement = normalize_statement(parsed, file_sha256)?;
        let lines = normalize_lines(&parsed.lines)?;
        let hashes = line_hashes(&statement.account_iban, &lines);

        let id = BankStatementId::generate();
        let mut tx = self.pool.begin().await.map_err(StoreError::Db)?;
        let inserted = sqlx::query(
            "INSERT INTO bank_statements (tenant_id, id, account_iban, currency, source, \
                 statement_ref, file_sha256, opening_balance_cents, closing_balance_cents, \
                 from_date, to_date, imported_by) \
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12) \
             ON CONFLICT (tenant_id, file_sha256) DO NOTHING",
        )
        .bind(self.tenant.as_str())
        .bind(id.as_str())
        .bind(&statement.account_iban)
        .bind(&statement.currency)
        .bind(statement.source.as_str())
        .bind(&statement.statement_ref)
        .bind(&statement.file_sha256)
        .bind(statement.opening_balance_cents)
        .bind(statement.closing_balance_cents)
        .bind(statement.from_date)
        .bind(statement.to_date)
        .bind(self.user.as_str())
        .execute(&mut *tx)
        .await
        .map_err(StoreError::Db)?;
        if inserted.rows_affected() == 0 {
            // Named by its period, not swallowed: a bookkeeper who uploads the
            // same file twice must be told where the first one went, or they
            // will look for the lines under today's date.
            let (from, to): (Date, Date) = sqlx::query_as(
                "SELECT from_date, to_date FROM bank_statements \
                 WHERE tenant_id = $1 AND file_sha256 = $2",
            )
            .bind(self.tenant.as_str())
            .bind(&statement.file_sha256)
            .fetch_one(&mut *tx)
            .await
            .map_err(StoreError::Db)?;
            return Err(StoreError::Conflict(format!(
                "this file has already been imported, as the statement of {from} to {to}"
            )));
        }

        let mut staged = 0usize;
        let mut duplicates = 0usize;
        for (index, (line, hash)) in lines.iter().zip(&hashes).enumerate() {
            let line_no = i32::try_from(index + 1).map_err(|_| too_many_lines())?;
            let written = sqlx::query(
                "INSERT INTO bank_lines (tenant_id, id, statement_id, line_no, booked_on, \
                     value_on, amount_cents, currency, counterparty_name, counterparty_iban, \
                     remittance, bank_ref, line_hash) \
                 VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13) \
                 ON CONFLICT (tenant_id, line_hash) DO NOTHING",
            )
            .bind(self.tenant.as_str())
            .bind(BankLineId::generate().as_str())
            .bind(id.as_str())
            .bind(line_no)
            .bind(line.booked_on)
            .bind(line.value_on)
            .bind(line.amount_cents)
            .bind(&line.currency)
            .bind(&line.counterparty_name)
            .bind(&line.counterparty_iban)
            .bind(&line.remittance)
            .bind(&line.bank_ref)
            .bind(hash)
            .execute(&mut *tx)
            .await
            .map_err(StoreError::Db)?;
            if written.rows_affected() == 0 {
                duplicates += 1;
            } else {
                staged += 1;
            }
        }
        tx.commit().await.map_err(StoreError::Db)?;

        let stored = self
            .bank_statement(&id)
            .await?
            .ok_or_else(|| StoreError::Db(sqlx::Error::RowNotFound))?;
        Ok(BankImport {
            statement: stored,
            staged,
            duplicates,
            unbooked: parsed.unbooked,
        })
    }

    /// This tenant's imports, the most recent period first.
    ///
    /// # Errors
    /// [`StoreError::Db`] on failure.
    pub async fn bank_statements(&self) -> Result<Vec<BankStatement>> {
        let rows = sqlx::query_as::<_, StatementRow>(&format!(
            "SELECT {STATEMENT_COLS} FROM bank_statements s WHERE s.tenant_id = $1 \
             ORDER BY s.to_date DESC, s.imported_at DESC, s.id"
        ))
        .bind(self.tenant.as_str())
        .fetch_all(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        rows.into_iter().map(StatementRow::into_statement).collect()
    }

    /// One of this tenant's imports, or `None` when the id is absent **or
    /// another tenant's** — never an existence oracle.
    ///
    /// # Errors
    /// [`StoreError::Db`] on failure.
    pub async fn bank_statement(&self, id: &BankStatementId) -> Result<Option<BankStatement>> {
        let row = sqlx::query_as::<_, StatementRow>(&format!(
            "SELECT {STATEMENT_COLS} FROM bank_statements s \
             WHERE s.tenant_id = $1 AND s.id = $2"
        ))
        .bind(self.tenant.as_str())
        .bind(id.as_str())
        .fetch_optional(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        row.map(StatementRow::into_statement).transpose()
    }

    /// One of this tenant's staged lines, or `None` when the id is absent **or
    /// another tenant's** — never an existence oracle, exactly like
    /// [`AccountStore::bank_statement`].
    ///
    /// The read reconciliation makes before it decides anything
    /// ([`crate::bank_reconcile`]): a line is matched by naming it, and naming
    /// somebody else's line has to be indistinguishable from naming one that
    /// was never imported.
    ///
    /// # Errors
    /// [`StoreError::Db`] on failure.
    pub async fn bank_line(&self, id: &BankLineId) -> Result<Option<BankLine>> {
        let row = sqlx::query_as::<_, LineRow>(&format!(
            "SELECT {LINE_COLS} FROM bank_lines WHERE tenant_id = $1 AND id = $2"
        ))
        .bind(self.tenant.as_str())
        .bind(id.as_str())
        .fetch_optional(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        row.map(LineRow::into_line).transpose()
    }

    /// This tenant's staged lines, oldest first — the order a bookkeeper works
    /// a month in — optionally narrowed to one import, one status, or both.
    ///
    /// An unknown or foreign `statement` yields an empty list rather than an
    /// error: the filter is a narrowing, and a narrowing that matches nothing
    /// matches nothing.
    ///
    /// # Errors
    /// [`StoreError::Db`] on failure.
    pub async fn bank_lines(
        &self,
        statement: Option<&BankStatementId>,
        status: Option<BankLineStatus>,
    ) -> Result<Vec<BankLine>> {
        let rows = sqlx::query_as::<_, LineRow>(&format!(
            "SELECT {LINE_COLS} FROM bank_lines \
             WHERE tenant_id = $1 \
               AND ($2::text IS NULL OR statement_id = $2) \
               AND ($3::text IS NULL OR status = $3) \
             ORDER BY booked_on, line_no, id \
             LIMIT $4"
        ))
        .bind(self.tenant.as_str())
        .bind(statement.map(BankStatementId::as_str))
        .bind(status.map(BankLineStatus::as_str))
        .bind(BANK_LINES_PAGE_MAX)
        .fetch_all(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        rows.into_iter().map(LineRow::into_line).collect()
    }
}

// ---- validation --------------------------------------------------------------

/// A validated, normalised statement header ready to be bound.
#[derive(Debug)]
struct NormalizedStatement {
    account_iban: String,
    currency: String,
    source: BankSource,
    statement_ref: String,
    file_sha256: String,
    opening_balance_cents: Option<i64>,
    closing_balance_cents: Option<i64>,
    from_date: Date,
    to_date: Date,
}

/// Validates and normalises a statement header. Pure — no database, so the
/// rules are unit-tested directly and one function runs for every format.
///
/// # Errors
/// [`StoreError::Validation`] naming the field that is wrong. Never the value:
/// a bank file is the tenant's own money moving, and error text is not a place
/// we put it.
fn normalize_statement(parsed: &ParsedStatement, file_sha256: &str) -> Result<NormalizedStatement> {
    let account_iban = match crate::iban::canonicalize(&parsed.account_iban) {
        Ok(Some(account)) => account,
        Ok(None) => {
            return Err(StoreError::Validation(
                "this statement names no account (IBAN), so we cannot tell whose money it is"
                    .to_owned(),
            ));
        }
        // The validator's own words, which name the rule and never echo the
        // number — exactly what a person needs to see and exactly what a log
        // must not (Law 1).
        Err(error) => {
            return Err(StoreError::Validation(format!(
                "this statement's account is not an IBAN we can read: {error}"
            )));
        }
    };
    if parsed.to_date < parsed.from_date {
        return Err(StoreError::Validation(
            "this statement ends before it starts, so its period is not a period".to_owned(),
        ));
    }
    if parsed.lines.len() > STATEMENT_LINES_MAX {
        return Err(too_many_lines());
    }
    for balance in [parsed.opening_balance_cents, parsed.closing_balance_cents]
        .into_iter()
        .flatten()
    {
        if balance.abs() > LINE_AMOUNT_MAX_CENTS {
            return Err(StoreError::Validation(
                "this statement states a balance too large to be one".to_owned(),
            ));
        }
    }
    Ok(NormalizedStatement {
        account_iban,
        currency: currency_code(&parsed.currency)?,
        source: parsed.source,
        statement_ref: clip(&parsed.statement_ref, STATEMENT_REF_MAX),
        file_sha256: hex_sha256(file_sha256)?,
        opening_balance_cents: parsed.opening_balance_cents,
        closing_balance_cents: parsed.closing_balance_cents,
        from_date: parsed.from_date,
        to_date: parsed.to_date,
    })
}

/// Validates and normalises the transactions.
///
/// Descriptive fields are **clipped**, money and identifiers are **checked**.
/// The asymmetry is the point: a counterparty name one character over ISO's
/// own limit is a cosmetic fact about a file we did not write, and refusing the
/// whole statement over it would lose every line in the month. An amount we
/// cannot hold, or a currency that is not a currency, is not cosmetic.
///
/// # Errors
/// [`StoreError::Validation`] naming the line number and what is wrong with it.
fn normalize_lines(lines: &[ParsedLine]) -> Result<Vec<ParsedLine>> {
    if lines.len() > STATEMENT_LINES_MAX {
        return Err(too_many_lines());
    }
    lines
        .iter()
        .enumerate()
        .map(|(index, line)| {
            let at = index + 1;
            if line.amount_cents == 0 {
                return Err(StoreError::Validation(format!(
                    "entry {at} of this statement moves nothing, which is not a transaction"
                )));
            }
            if line.amount_cents.abs() > LINE_AMOUNT_MAX_CENTS {
                return Err(StoreError::Validation(format!(
                    "entry {at} of this statement states an amount too large to be one"
                )));
            }
            let currency = currency_code(&line.currency).map_err(|_| {
                StoreError::Validation(format!(
                    "entry {at} of this statement names no currency we can read: a currency is a \
                     three-letter ISO 4217 code"
                ))
            })?;
            Ok(ParsedLine {
                booked_on: line.booked_on,
                value_on: line.value_on,
                amount_cents: line.amount_cents,
                currency,
                counterparty_name: clip(&line.counterparty_name, COUNTERPARTY_NAME_MAX),
                // An identifier we cannot read is absent, never clipped and
                // never a refusal: half an IBAN is not a shorter IBAN, it is a
                // wrong one, and B4.09 matches on this field — while a
                // counterparty's mistyped account is the bank's problem with
                // one transaction, not a reason to lose the month.
                counterparty_iban: counterparty_iban(&line.counterparty_iban),
                remittance: clip(&line.remittance, REMITTANCE_MAX),
                bank_ref: clip(&line.bank_ref, BANK_REF_MAX),
            })
        })
        .collect()
}

/// The refusal for a file with more transactions than we will stage.
fn too_many_lines() -> StoreError {
    StoreError::Validation(format!(
        "a statement file may state at most {STATEMENT_LINES_MAX} transactions; split it by period"
    ))
}

/// The counterparty's account in canonical form, or `""` when the file states
/// none or states something that is not an IBAN.
///
/// [`crate::iban`] is the crate's one notion of what an IBAN is — shape, the
/// per-country length and the ISO 7064 check digits — and reusing it here is
/// what keeps a bank line's account and an invoice's payee account the same
/// kind of string. The difference is only what a failure means: on an invoice
/// the tenant is typing and is told; here a bank is reporting, and an
/// unreadable counterparty account is one blank field, not a lost statement.
fn counterparty_iban(raw: &str) -> String {
    crate::iban::canonicalize(raw)
        .ok()
        .flatten()
        .unwrap_or_default()
}

/// Keeps at most `max` characters of a descriptive field, trimmed.
fn clip(raw: &str, max: usize) -> String {
    let text = raw.trim();
    match text.char_indices().nth(max) {
        None => text.to_owned(),
        Some((end, _)) => text[..end].trim_end().to_owned(),
    }
}

/// Checks a lowercase hex SHA-256.
///
/// # Errors
/// [`StoreError::Validation`] when it is not one — which can only mean a bug in
/// our own caller, since every door computes it from the bytes.
fn hex_sha256(value: &str) -> Result<String> {
    let ok = value.len() == 64
        && value
            .bytes()
            .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase());
    if !ok {
        return Err(StoreError::Validation(
            "a statement's file digest must be a lowercase hex SHA-256".to_owned(),
        ));
    }
    Ok(value.to_owned())
}

/// The SHA-256 of some bytes, lowercase hex.
///
/// `pub(crate)` for [`crate::bank_read`], which owns the door all three formats
/// arrive through and therefore owns the bytes the digest is of.
pub(crate) fn sha256_hex(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    digest.iter().map(|byte| format!("{byte:02x}")).collect()
}

// ---- the line hash -----------------------------------------------------------

/// The duplicate key for every line of one statement, in line order.
///
/// Computed for the whole file at once because of the **occurrence number**:
/// two genuinely distinct transactions can be identical in every field a bank
/// states — two €3.40 coffees at the same shop on the same day, with no bank
/// reference between them — and hashing content alone would silently drop the
/// second one for ever. So the n-th line with identical content carries `n` in
/// its hash. Re-importing the same file re-derives the same numbers and
/// de-duplicates exactly; an overlapping file lists the same pair in the same
/// order and does too.
fn line_hashes(account_iban: &str, lines: &[ParsedLine]) -> Vec<String> {
    let mut seen: HashMap<String, usize> = HashMap::new();
    lines
        .iter()
        .map(|line| {
            let content = line_content(account_iban, line);
            let occurrence = seen.entry(content.clone()).or_insert(0);
            *occurrence += 1;
            line_hash(&content, *occurrence)
        })
        .collect()
}

/// Everything about a line that makes it *that* line, normalised.
///
/// The value date is deliberately **not** in it: some banks restate it when a
/// booking is corrected, and a line whose hash moves is a line that imports
/// twice. The booked date, the signed amount, the currency, the bank's
/// reference, the counterparty's account and what was written on it are what
/// stay put.
fn line_content(account_iban: &str, line: &ParsedLine) -> String {
    format!(
        "{}|{}|{}|{}|{}|{}|{}",
        account_iban,
        line.booked_on,
        line.amount_cents,
        line.currency,
        squash(&line.bank_ref),
        squash(&line.counterparty_iban),
        squash(&line.remittance),
    )
}

/// The hash of one line's content at its `occurrence`-th appearance.
fn line_hash(content: &str, occurrence: usize) -> String {
    sha256_hex(format!("{content}|{occurrence}").as_bytes())
}

/// Lowercases and collapses whitespace, so that a bank reformatting its own
/// remittance across two exports does not turn one transaction into two.
fn squash(raw: &str) -> String {
    raw.split_whitespace()
        .map(str::to_lowercase)
        .collect::<Vec<_>>()
        .join(" ")
}

// ---- rows --------------------------------------------------------------------

/// One row of `bank_statements`, in [`STATEMENT_COLS`] order.
#[derive(sqlx::FromRow)]
struct StatementRow {
    id: String,
    account_iban: String,
    currency: String,
    source: String,
    statement_ref: String,
    file_sha256: String,
    opening_balance_cents: Option<i64>,
    closing_balance_cents: Option<i64>,
    from_date: Date,
    to_date: Date,
    imported_by: String,
    imported_at: OffsetDateTime,
    line_count: i64,
}

impl StatementRow {
    /// The stored statement.
    ///
    /// # Errors
    /// [`StoreError::Db`] when the row names a source the code does not know —
    /// a decode failure rather than a guess, because the source decides how a
    /// blank field is read.
    fn into_statement(self) -> Result<BankStatement> {
        let source = BankSource::parse(&self.source).ok_or_else(|| {
            StoreError::Db(sqlx::Error::Decode(
                "bank_statements.source is not a known source".into(),
            ))
        })?;
        Ok(BankStatement {
            id: BankStatementId::new(self.id),
            account_iban: self.account_iban,
            currency: self.currency,
            source,
            statement_ref: self.statement_ref,
            file_sha256: self.file_sha256,
            opening_balance_cents: self.opening_balance_cents,
            closing_balance_cents: self.closing_balance_cents,
            from_date: self.from_date,
            to_date: self.to_date,
            imported_by: UserId::new(self.imported_by),
            imported_at: self.imported_at,
            line_count: self.line_count,
        })
    }
}

/// One row of `bank_lines`, in [`LINE_COLS`] order.
#[derive(sqlx::FromRow)]
struct LineRow {
    id: String,
    statement_id: String,
    line_no: i32,
    booked_on: Date,
    value_on: Date,
    amount_cents: i64,
    currency: String,
    counterparty_name: String,
    counterparty_iban: String,
    remittance: String,
    bank_ref: String,
    status: String,
    ignored_reason: String,
    created_at: OffsetDateTime,
}

impl LineRow {
    /// The stored line.
    ///
    /// # Errors
    /// [`StoreError::Db`] when the row carries a status the code does not know
    /// — guessing here would decide whether money has already been booked.
    fn into_line(self) -> Result<BankLine> {
        let status = BankLineStatus::parse(&self.status).ok_or_else(|| {
            StoreError::Db(sqlx::Error::Decode(
                "bank_lines.status is not a known status".into(),
            ))
        })?;
        Ok(BankLine {
            id: BankLineId::new(self.id),
            statement_id: BankStatementId::new(self.statement_id),
            line_no: self.line_no,
            booked_on: self.booked_on,
            value_on: self.value_on,
            amount_cents: self.amount_cents,
            currency: self.currency,
            counterparty_name: self.counterparty_name,
            counterparty_iban: self.counterparty_iban,
            remittance: self.remittance,
            bank_ref: self.bank_ref,
            status,
            ignored_reason: self.ignored_reason,
            created_at: self.created_at,
        })
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used)]

    use super::*;
    use time::Month;

    fn day(year: i32, month: Month, day: u8) -> Date {
        Date::from_calendar_date(year, month, day).unwrap_or(Date::MIN)
    }

    fn line(amount_cents: i64, remittance: &str, bank_ref: &str) -> ParsedLine {
        ParsedLine {
            booked_on: day(2026, Month::January, 5),
            value_on: day(2026, Month::January, 5),
            amount_cents,
            currency: "EUR".to_owned(),
            counterparty_name: "Kaffeehaus".to_owned(),
            counterparty_iban: String::new(),
            remittance: remittance.to_owned(),
            bank_ref: bank_ref.to_owned(),
        }
    }

    fn statement(lines: Vec<ParsedLine>) -> ParsedStatement {
        ParsedStatement {
            source: BankSource::Camt,
            account_iban: "DE02 1203 0000 0000 2020 51".to_owned(),
            currency: "EUR".to_owned(),
            statement_ref: "2026/001".to_owned(),
            opening_balance_cents: Some(100_000),
            closing_balance_cents: Some(90_000),
            from_date: day(2026, Month::January, 1),
            to_date: day(2026, Month::January, 31),
            lines,
            unbooked: 0,
        }
    }

    fn sha() -> String {
        sha256_hex(b"a statement")
    }

    fn refused<T: std::fmt::Debug>(result: Result<T>) -> String {
        match result {
            Err(StoreError::Validation(message)) => message,
            other => panic!("expected a Validation refusal, got {other:?}"),
        }
    }

    #[test]
    fn a_counterparty_account_is_canonical_or_it_is_nothing() {
        assert_eq!(
            counterparty_iban("de02 1203 0000 0000 2020 51"),
            "DE02120300000000202051",
            "spaces and case are layout"
        );
        // A bank stating something we cannot read leaves the field blank
        // rather than losing the statement: too short, wrong check digits,
        // punctuation, no country prefix.
        for unreadable in [
            "",
            "DE0212030",
            "DE03120300000000202051",
            "DE02-1203-0000-0000-2020-51",
            "1234567890123456",
        ] {
            assert_eq!(counterparty_iban(unreadable), "", "for {unreadable:?}");
        }
    }

    #[test]
    fn the_statement_header_is_normalised_and_its_period_is_checked() {
        let normalized = normalize_statement(&statement(vec![]), &sha()).expect("a valid header");
        assert_eq!(normalized.account_iban, "DE02120300000000202051");
        assert_eq!(normalized.currency, "EUR");
        assert_eq!(normalized.statement_ref, "2026/001");

        let mut backwards = statement(vec![]);
        backwards.to_date = day(2025, Month::December, 1);
        assert!(
            refused(normalize_statement(&backwards, &sha())).contains("not a period"),
            "a statement that ends before it starts is refused"
        );

        let mut nameless = statement(vec![]);
        nameless.account_iban = "not an account".to_owned();
        assert!(refused(normalize_statement(&nameless, &sha())).contains("IBAN"));
    }

    #[test]
    fn a_balance_the_file_did_not_state_is_absent_not_zero() {
        let mut silent = statement(vec![]);
        silent.opening_balance_cents = None;
        silent.closing_balance_cents = None;
        let normalized = normalize_statement(&silent, &sha()).expect("a valid header");
        assert_eq!(normalized.opening_balance_cents, None);
        assert_eq!(normalized.closing_balance_cents, None);
    }

    #[test]
    fn a_line_that_moves_nothing_or_too_much_is_refused_by_its_number() {
        let zero = refused(normalize_lines(&[line(1_00, "a", ""), line(0, "b", "")]));
        assert!(
            zero.contains("entry 2"),
            "the refusal names the line: {zero}"
        );
        let huge = refused(normalize_lines(&[line(LINE_AMOUNT_MAX_CENTS + 1, "a", "")]));
        assert!(huge.contains("entry 1") && huge.contains("too large"));

        let mut foreign = line(1_00, "a", "");
        foreign.currency = "euro".to_owned();
        let bad = refused(normalize_lines(&[foreign]));
        assert!(bad.contains("entry 1") && bad.contains("ISO 4217"));
    }

    #[test]
    fn descriptive_fields_are_clipped_and_a_half_read_iban_is_dropped() {
        let mut long = line(1_00, &"x".repeat(REMITTANCE_MAX + 50), "");
        long.counterparty_name = "y".repeat(COUNTERPARTY_NAME_MAX + 10);
        long.counterparty_iban = "DE02 12".to_owned();
        let normalized = normalize_lines(&[long]).expect("clipped, not refused");
        assert_eq!(normalized[0].remittance.chars().count(), REMITTANCE_MAX);
        assert_eq!(
            normalized[0].counterparty_name.chars().count(),
            COUNTERPARTY_NAME_MAX
        );
        assert_eq!(
            normalized[0].counterparty_iban, "",
            "half an IBAN is not a shorter IBAN"
        );
    }

    #[test]
    fn two_identical_transactions_on_one_day_are_two_lines() {
        let coffees = vec![line(-3_40, "Kaffee", ""), line(-3_40, "Kaffee", "")];
        let hashes = line_hashes("DE02", &coffees);
        assert_ne!(
            hashes[0], hashes[1],
            "the second coffee must not vanish as a duplicate"
        );
        // And re-reading the same file derives the same two hashes, so the
        // second import stages neither.
        assert_eq!(hashes, line_hashes("DE02", &coffees));
    }

    #[test]
    fn a_reformatted_remittance_is_the_same_transaction() {
        let one = line(-3_40, "INV-2026-00007  Vielen Dank", "REF9");
        let other = line(-3_40, "inv-2026-00007 vielen  dank", "REF9");
        assert_eq!(
            line_hashes("DE02", &[one])[0],
            line_hashes("DE02", &[other])[0]
        );
    }

    #[test]
    fn the_hash_moves_with_everything_that_makes_a_line_that_line() {
        let base = line(-3_40, "Kaffee", "REF9");
        let hash = line_hashes("DE02", std::slice::from_ref(&base))[0].clone();

        let mut other_amount = base.clone();
        other_amount.amount_cents = -3_41;
        let mut other_sign = base.clone();
        other_sign.amount_cents = 3_40;
        let mut other_day = base.clone();
        other_day.booked_on = day(2026, Month::January, 6);
        let mut other_ref = base.clone();
        other_ref.bank_ref = "REF10".to_owned();
        let mut other_party = base.clone();
        other_party.counterparty_iban = "NL91ABNA0417164300".to_owned();
        let mut other_currency = base.clone();
        other_currency.currency = "CHF".to_owned();
        for changed in [
            other_amount,
            other_sign,
            other_day,
            other_ref,
            other_party,
            other_currency,
        ] {
            assert_ne!(hash, line_hashes("DE02", &[changed])[0]);
        }

        // The value date is not in it: a bank that restates it has not sent us
        // a second transaction.
        let mut revalued = base.clone();
        revalued.value_on = day(2026, Month::January, 9);
        assert_eq!(hash, line_hashes("DE02", &[revalued])[0]);

        // And the same transaction on another of the tenant's accounts is
        // another transaction.
        assert_ne!(hash, line_hashes("NL91", &[base])[0]);
    }

    #[test]
    fn a_file_bigger_than_the_cap_is_refused_by_the_count() {
        let many: Vec<ParsedLine> = (0..=STATEMENT_LINES_MAX)
            .map(|n| line(-1_00, &format!("line {n}"), ""))
            .collect();
        assert!(refused(normalize_lines(&many)).contains("at most"));
        assert!(refused(normalize_statement(&statement(many), &sha())).contains("at most"));
    }

    #[test]
    fn the_stored_words_round_trip() {
        for source in [BankSource::Camt, BankSource::Mt940, BankSource::Csv] {
            assert_eq!(BankSource::parse(source.as_str()), Some(source));
        }
        for status in [
            BankLineStatus::Unmatched,
            BankLineStatus::Matched,
            BankLineStatus::Ignored,
        ] {
            assert_eq!(BankLineStatus::parse(status.as_str()), Some(status));
        }
        assert_eq!(BankSource::parse("swift"), None);
        assert_eq!(BankLineStatus::parse("paid"), None);
    }

    #[test]
    fn a_digest_that_is_not_one_is_refused() {
        assert_eq!(sha256_hex(b"").len(), 64);
        assert!(hex_sha256(&sha256_hex(b"alo")).is_ok());
        assert!(refused(hex_sha256("")).contains("SHA-256"));
        assert!(hex_sha256(&sha256_hex(b"alo").to_uppercase()).is_err());
    }
}
