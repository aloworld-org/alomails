//! Importing leads from a spreadsheet (alo CRM, ADR 0035, wave B2.09) — a
//! file of rows becomes deals on a board, or it becomes a report saying why it
//! did not.
//!
//! The whole item is one question asked twice: *what would this file do?*
//! [`AccountStore::preview_crm_lead_import`] answers it and writes nothing;
//! [`AccountStore::import_crm_leads`] answers it — by calling the preview — and
//! then, if every row is importable, writes the deals in **one transaction**. The two share every
//! rule by construction, so the preview cannot promise something the commit
//! then refuses.
//!
//! Four decisions shape this file.
//!
//! - **All-or-nothing.** A partial import leaves a person guessing which half
//!   landed, and re-importing to find out doubles the good half. The preview
//!   already names every blocking row, so refusing the whole file costs one fix
//!   and one retry (`docs/design/crm.md` § Importing leads).
//! - **A duplicate is not a failure.** A row that names somebody the tenant
//!   already knows is *skipped and reported*, and the rest of the file
//!   proceeds. Merging is not attempted: a merge tool is its own item once
//!   there is real data to merge.
//! - **The domain rule stops at free mail.** A lead is a duplicate when its
//!   address is already known, or when its **company domain** already belongs
//!   to a customer or an open deal. Half of European SME contacts write from
//!   Gmail, so matching on a free-mail domain would fold every unrelated
//!   consumer lead into one — [`crate::crm_thread_match::is_free_mail_domain`]
//!   is the same list the thread suggestions live by, and it is consulted here
//!   for the same reason.
//! - **Money is never guessed.** `1.234,56`, `1,234.56` and `1234.56` all mean
//!   one thing and are read exactly ([`parse_value_cents`]); `1.234` means two
//!   different things in two European countries and is **refused as ambiguous**
//!   rather than imported as either. Cents are integers from the file to the
//!   column, and no float is used anywhere in the reading.
//!
//! What the file may not decide: who owns the deals (the importing user), what
//! stage they land in (the caller's, or the board's first column), and whether
//! any of them is closed (none is — a deal that was never worked was never
//! won).

use std::collections::HashSet;

use time::Date;
use time::format_description::well_known::Iso8601;

use crate::account::AccountStore;
use crate::billing_field::{DEFAULT_CURRENCY, bounded, currency, required};
use crate::crm_deals::{
    DEAL_EMAIL_MAX_CHARS, DEAL_PARTY_MAX_CHARS, DEAL_SOURCE_MAX_CHARS, DEAL_TITLE_MAX_CHARS,
    DEAL_VALUE_MAX_CENTS, NewDeal,
};
use crate::crm_thread_match::{domain_of, is_free_mail_domain};
use crate::csv_read::{CsvRow, CsvTable, parse as parse_csv};
use crate::error::{Result, StoreError};
use crate::id::{CrmDealId, CrmPipelineId, CrmStageId};
use crate::money_text::{AmountText, parse_amount_cents};

/// The most bytes an uploaded lead list may be: 2 MiB, which is tens of
/// thousands of rows of text and far more than [`MAX_IMPORT_ROWS`] allows.
pub const MAX_IMPORT_BYTES: usize = 2 * 1024 * 1024;

/// The most rows one file may carry. An import is a person's list, not a data
/// migration: two thousand rows is a very large one, and the cap is what keeps
/// a single transaction bounded.
pub const MAX_IMPORT_ROWS: usize = 2_000;

/// Which column of the file carries which field of a deal.
///
/// Every field is a **column name** as it appears in the header, matched
/// case- and space-insensitively, and every one is optional: a file with
/// nothing but a company column imports leads with a company and no more. The
/// caller states the mapping; [`LeadMapping::infer`] is what a client shows as
/// a first guess for a person to correct.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LeadMapping {
    /// What the opportunity is called. Falls back to the company name.
    pub title: Option<String>,
    /// The company the lead is at.
    pub company: Option<String>,
    /// Who at the company is being spoken to.
    pub contact_name: Option<String>,
    /// Their email address — the field every duplicate rule turns on.
    pub email: Option<String>,
    /// What the opportunity is worth, as an amount in the deal's currency.
    pub value: Option<String>,
    /// ISO 4217 code per row; the tenant's default when the file states none.
    pub currency: Option<String>,
    /// The day it is expected to close, `YYYY-MM-DD`.
    pub expected_close: Option<String>,
    /// Where the lead came from, in the tenant's own vocabulary.
    pub source: Option<String>,
}

/// The header names each field is guessed from, in English, French and Dutch —
/// the three languages the product ships strings in. Folded the same way
/// [`CsvTable::column`] folds a header, so `E-Mail` and `email` are one word.
const GUESSES: [(&str, &[&str]); 8] = [
    ("title", &["title", "opportunity", "deal", "titre", "titel"]),
    (
        "company",
        &[
            "company",
            "companyname",
            "organisation",
            "organization",
            "account",
            "société",
            "societe",
            "entreprise",
            "bedrijf",
        ],
    ),
    (
        "contact",
        &[
            "contact",
            "contactname",
            "name",
            "fullname",
            "nom",
            "naam",
            "contactpersoon",
        ],
    ),
    (
        "email",
        &["email", "emailaddress", "mail", "courriel", "epost"],
    ),
    (
        "value",
        &[
            "value", "amount", "worth", "revenue", "montant", "valeur", "waarde", "bedrag",
        ],
    ),
    ("currency", &["currency", "devise", "valuta", "munt"]),
    (
        "close",
        &[
            "expectedclose",
            "closedate",
            "close",
            "expected",
            "clôture",
            "cloture",
            "afsluitdatum",
        ],
    ),
    (
        "source",
        &["source", "origin", "herkomst", "bron", "origine"],
    ),
];

impl LeadMapping {
    /// The mapping a header suggests: for each field, the first column whose
    /// name is one of the words this product knows for it.
    ///
    /// A guess, offered to a person to correct — never applied silently to a
    /// commit the person did not preview.
    #[must_use]
    pub fn infer(table: &CsvTable) -> Self {
        let pick = |key: &str| {
            let words = GUESSES
                .iter()
                .find(|(field, _)| *field == key)
                .map(|(_, words)| *words)
                .unwrap_or_default();
            words
                .iter()
                .find_map(|word| table.column(word))
                .map(|at| table.header[at].clone())
        };
        Self {
            title: pick("title"),
            company: pick("company"),
            contact_name: pick("contact"),
            email: pick("email"),
            value: pick("value"),
            currency: pick("currency"),
            expected_close: pick("close"),
            source: pick("source"),
        }
    }

    /// Whether the caller stated nothing at all, in which case the header's own
    /// guess is used.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        *self == Self::default()
    }

    /// This mapping resolved against a header, refusing any column name the
    /// file does not have — a mapping that quietly points at nothing would
    /// import a file of blank leads.
    fn resolve(&self, table: &CsvTable) -> Result<Columns> {
        let at = |field: &str, name: &Option<String>| match name {
            None => Ok(None),
            Some(name) => table.column(name).map(Some).ok_or_else(|| {
                StoreError::Validation(format!("the file has no column mapped to {field}"))
            }),
        };
        Ok(Columns {
            title: at("title", &self.title)?,
            company: at("company name", &self.company)?,
            contact_name: at("contact name", &self.contact_name)?,
            email: at("email", &self.email)?,
            value: at("value", &self.value)?,
            currency: at("currency", &self.currency)?,
            expected_close: at("expected close", &self.expected_close)?,
            source: at("source", &self.source)?,
        })
    }
}

/// The mapping as column indices.
#[derive(Debug, Clone, Copy, Default)]
struct Columns {
    title: Option<usize>,
    company: Option<usize>,
    contact_name: Option<usize>,
    email: Option<usize>,
    value: Option<usize>,
    currency: Option<usize>,
    expected_close: Option<usize>,
    source: Option<usize>,
}

/// Why a row was skipped.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DuplicateReason {
    /// This exact address is already known.
    Email,
    /// The address's domain is already a customer's or an open deal's, and it
    /// is not a free-mail domain.
    Domain,
}

impl DuplicateReason {
    /// The word the surface reports.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Email => "email",
            Self::Domain => "domain",
        }
    }
}

/// Where the thing it matched already stood.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DuplicateSource {
    /// A customer or an open deal the tenant already has.
    Crm,
    /// An earlier row of this same file.
    File,
}

impl DuplicateSource {
    /// The word the surface reports.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Crm => "crm",
            Self::File => "file",
        }
    }
}

/// A row that would not be imported because the tenant (or the file) already
/// knows it.
#[derive(Debug, Clone)]
pub struct DuplicateRow {
    /// The line of the file, as a spreadsheet numbers it.
    pub line: usize,
    /// Which rule matched.
    pub reason: DuplicateReason,
    /// Whether the match was already in CRM or earlier in this file.
    pub source: DuplicateSource,
    /// The address or the domain that matched — the row's own value, given
    /// back to the person who uploaded it so the skip is checkable.
    pub matched: String,
}

/// A row that cannot be imported at all, with the rule it broke.
#[derive(Debug, Clone)]
pub struct RowError {
    /// The line of the file.
    pub line: usize,
    /// The rule, in the store's own words. Never the row's content: a lead
    /// list is somebody's customer data (law 1).
    pub rule: String,
}

/// A lead the import would create, or did.
#[derive(Debug, Clone)]
pub struct LeadRow {
    /// The line of the file.
    pub line: usize,
    /// The deal as it will be written — validated, trimmed, in integer cents.
    pub deal: NewDeal,
    /// The id it was written under, or `None` on a preview.
    pub id: Option<CrmDealId>,
}

/// What a file would do, or did.
#[derive(Debug, Clone)]
pub struct LeadImportReport {
    /// Whether the deals were written. `false` on every preview, and on a
    /// commit refused because a row was invalid.
    pub committed: bool,
    /// How the bytes were decoded.
    pub encoding: &'static str,
    /// The delimiter that was sniffed.
    pub delimiter: char,
    /// The header, in file order — what a client builds its mapping picker
    /// from.
    pub columns: Vec<String>,
    /// The mapping actually used: the caller's, or the header's own guess.
    pub mapping: LeadMapping,
    /// Data rows read from the file (blank lines are not rows).
    pub total_rows: usize,
    /// The leads, in file order.
    pub leads: Vec<LeadRow>,
    /// The rows skipped as duplicates, in file order.
    pub duplicates: Vec<DuplicateRow>,
    /// The rows that cannot be imported, in file order.
    pub errors: Vec<RowError>,
}

/// What a caller sends with the file.
#[derive(Debug, Clone)]
pub struct LeadImportRequest {
    /// The board the leads land on.
    pub pipeline_id: CrmPipelineId,
    /// The column they land in, or `None` for the board's first live column.
    pub stage_id: Option<CrmStageId>,
    /// Which column of the file is which field. Empty means "guess".
    pub mapping: LeadMapping,
}

/// Everything known about the tenant that decides whether a row is a
/// duplicate, read once per import.
///
/// Sets, not lists: a tenant with ten thousand customers is answered in
/// constant time per row rather than scanning the lot two thousand times.
#[derive(Debug, Default)]
struct Known {
    /// Lowercased addresses of customers and open deals.
    addresses: HashSet<String>,
    /// Lowercased domains of those addresses, free mail excluded.
    domains: HashSet<String>,
}

impl Known {
    /// Whether an address, or its domain, is already spoken for.
    fn duplicate_of(&self, address: &str) -> Option<DuplicateReason> {
        let lower = address.to_ascii_lowercase();
        if self.addresses.contains(&lower) {
            return Some(DuplicateReason::Email);
        }
        let domain = domain_of(&lower)?;
        if !is_free_mail_domain(domain) && self.domains.contains(domain) {
            return Some(DuplicateReason::Domain);
        }
        None
    }

    /// Remembers an address the import is about to create, so a file that
    /// lists one company twice creates it once.
    fn remember(&mut self, address: &str) {
        let lower = address.to_ascii_lowercase();
        if let Some(domain) = domain_of(&lower)
            && !is_free_mail_domain(domain)
        {
            self.domains.insert(domain.to_owned());
        }
        self.addresses.insert(lower);
    }
}

impl AccountStore {
    /// What a file **would** do, writing nothing.
    ///
    /// The read runs in a transaction it then rolls back, so the duplicate
    /// rules see one consistent picture of the tenant — the same picture the
    /// commit will take its own snapshot of.
    ///
    /// # Errors
    /// [`StoreError::NotFound`] when the board or the column is not this
    /// tenant's; [`StoreError::Validation`] when the file is not readable CSV,
    /// has no header, breaks a cap, or is mapped to a column it does not have;
    /// [`StoreError::Db`] on failure.
    pub async fn preview_crm_lead_import(
        &self,
        request: &LeadImportRequest,
        file: &[u8],
    ) -> Result<LeadImportReport> {
        let table = read_file(file)?;
        let mut tx = self.pool.begin().await.map_err(StoreError::Db)?;
        // The board is resolved exactly as the commit resolves it, so a
        // preview cannot promise a landing place the commit would refuse —
        // and a board that is not this tenant's is the `NotFound` an id that
        // never existed gets, before a single row is read.
        self.share_crm_pipeline(&mut tx, &request.pipeline_id)
            .await?;
        self.import_target_stage(&mut tx, &request.pipeline_id, request.stage_id.as_ref())
            .await?;
        let known = self.known_parties(&mut tx).await?;
        // Nothing was written; rolling back explicitly says so.
        tx.rollback().await.map_err(StoreError::Db)?;
        classify(&table, &request.mapping, known)
    }

    /// Imports the file: every lead, or none of them.
    ///
    /// A row that cannot be imported refuses the **whole** file, and the report
    /// comes back with `committed: false` and every broken rule named; the
    /// route edge turns that into the `422` the design note publishes. Rows
    /// skipped as duplicates are not failures and do not block the rest.
    ///
    /// **The duplicate rules read a snapshot taken a moment before the write.**
    /// Two people importing overlapping files in the same second can therefore
    /// each create a lead the other was about to: being a duplicate is a rule
    /// about *tidiness*, not an invariant, and the alternative — holding the
    /// board exclusively for the length of an upload — would block every card
    /// move on it to prevent a duplicate a person can archive.
    ///
    /// # Errors
    /// As [`AccountStore::preview_crm_lead_import`].
    pub async fn import_crm_leads(
        &self,
        request: &LeadImportRequest,
        file: &[u8],
    ) -> Result<LeadImportReport> {
        // The reading is the preview's, verbatim — one function, so the two
        // answers cannot disagree about what a file says.
        let mut report = self.preview_crm_lead_import(request, file).await?;
        if !report.errors.is_empty() {
            // Nothing is written, and the report says which rows to fix.
            return Ok(report);
        }
        // Validated before the transaction is opened, never inside it: an
        // open transaction waiting on a second pooled connection is how a
        // busy server deadlocks (`crm_deals::Normalized`).
        let mut ready = Vec::with_capacity(report.leads.len());
        for lead in &report.leads {
            ready.push(self.normalize_deal(&lead.deal).await?);
        }
        let mut tx = self.pool.begin().await.map_err(StoreError::Db)?;
        // The board is held shared for the whole write, exactly as one card
        // move holds it: an import may not slip past a column being archived.
        self.share_crm_pipeline(&mut tx, &request.pipeline_id)
            .await?;
        let stage = self
            .import_target_stage(&mut tx, &request.pipeline_id, request.stage_id.as_ref())
            .await?;
        for (lead, deal) in report.leads.iter_mut().zip(&ready) {
            let id = self
                .insert_crm_deal_in(&mut tx, &request.pipeline_id, &stage, deal)
                .await?;
            lead.id = Some(id);
        }
        tx.commit().await.map_err(StoreError::Db)?;
        report.committed = true;
        Ok(report)
    }

    /// The column the leads land in: the one the caller named (this board's,
    /// not archived), or the board's first live column.
    async fn import_target_stage(
        &self,
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
        pipeline: &CrmPipelineId,
        stage: Option<&CrmStageId>,
    ) -> Result<CrmStageId> {
        if let Some(stage) = stage {
            // Resolved by the same rules a card move is — this tenant's, this
            // board's, not archived — and by the same code, so an import can
            // never land somewhere a move could not.
            let target = self.resolve_target_stage(tx, pipeline, stage).await?;
            return Ok(CrmStageId::new(target.id));
        }
        let first: Option<String> = sqlx::query_scalar(
            "SELECT id FROM crm_stages \
             WHERE tenant_id = $1 AND pipeline_id = $2 AND archived_at IS NULL \
             ORDER BY position, created_at, id LIMIT 1",
        )
        .bind(self.tenant.as_str())
        .bind(pipeline.as_str())
        .fetch_optional(&mut **tx)
        .await
        .map_err(StoreError::Db)?;
        first.map(CrmStageId::new).ok_or_else(|| {
            StoreError::Validation(
                "this board has no stage to import into; add one first".to_owned(),
            )
        })
    }

    /// Every address the tenant already knows: its customers', and the
    /// contacts on its **open** deals. A closed deal is history, and history
    /// must not make tomorrow's lead a duplicate.
    async fn known_parties(&self, tx: &mut sqlx::Transaction<'_, sqlx::Postgres>) -> Result<Known> {
        let rows: Vec<(String,)> = sqlx::query_as(
            "SELECT lower(email) FROM billing_customers \
             WHERE tenant_id = $1 AND email <> '' \
             UNION \
             SELECT lower(contact_email) FROM crm_deals \
             WHERE tenant_id = $1 AND contact_email <> '' AND outcome IS NULL",
        )
        .bind(self.tenant.as_str())
        .fetch_all(&mut **tx)
        .await
        .map_err(StoreError::Db)?;
        let mut known = Known::default();
        for (address,) in rows {
            known.remember(&address);
        }
        Ok(known)
    }
}

/// Reads the uploaded bytes as CSV, holding them to the size cap first — a
/// refusal a caller can act on before anything is decoded.
fn read_file(file: &[u8]) -> Result<CsvTable> {
    if file.is_empty() {
        return Err(StoreError::Validation("the file is empty".to_owned()));
    }
    if file.len() > MAX_IMPORT_BYTES {
        return Err(StoreError::Validation(format!(
            "the file is larger than {} MiB; split it",
            MAX_IMPORT_BYTES / (1024 * 1024)
        )));
    }
    parse_csv(file, MAX_IMPORT_ROWS)
}

/// The whole reading of a file: what each row would become, what is a
/// duplicate of what, and what cannot be imported at all.
///
/// Pure — it is given the tenant's known parties rather than reading them — so
/// every rule below is unit-testable without a database, and the preview and
/// the commit cannot drift apart.
fn classify(table: &CsvTable, mapping: &LeadMapping, mut known: Known) -> Result<LeadImportReport> {
    let mapping = if mapping.is_empty() {
        LeadMapping::infer(table)
    } else {
        mapping.clone()
    };
    let columns = mapping.resolve(table)?;
    // A file nothing is mapped from would import a page of blank leads, or —
    // once the row rules ran — the same refusal repeated two thousand times.
    // One sentence naming what is missing is what a person can act on, and it
    // is what the preview screen exists to let them fix.
    if columns.title.is_none() && columns.company.is_none() {
        return Err(StoreError::Validation(
            "no column is mapped to a title or a company name; say which column holds it"
                .to_owned(),
        ));
    }
    let mut report = LeadImportReport {
        committed: false,
        encoding: table.encoding.as_str(),
        delimiter: table.delimiter,
        columns: table.header.clone(),
        mapping,
        total_rows: table.rows.len(),
        leads: Vec::new(),
        duplicates: Vec::new(),
        errors: Vec::new(),
    };
    for row in &table.rows {
        match lead_from_row(row, &columns) {
            Err(StoreError::Validation(rule)) => report.errors.push(RowError {
                line: row.line,
                rule,
            }),
            Err(other) => return Err(other),
            Ok(deal) => {
                let address = deal.contact_email.clone();
                if !address.is_empty()
                    && let Some(reason) = known.duplicate_of(&address)
                {
                    let matched = match reason {
                        DuplicateReason::Email => address.to_ascii_lowercase(),
                        DuplicateReason::Domain => {
                            domain_of(&address).unwrap_or_default().to_ascii_lowercase()
                        }
                    };
                    // "Already in CRM" or "twice in this file" is decided by
                    // asking the leads already taken from this file, which are
                    // the only things remembered since the tenant was read.
                    let source = if report
                        .leads
                        .iter()
                        .any(|lead| matches_lead(&lead.deal.contact_email, &address, reason))
                    {
                        DuplicateSource::File
                    } else {
                        DuplicateSource::Crm
                    };
                    report.duplicates.push(DuplicateRow {
                        line: row.line,
                        reason,
                        source,
                        matched,
                    });
                    continue;
                }
                if !address.is_empty() {
                    known.remember(&address);
                }
                report.leads.push(LeadRow {
                    line: row.line,
                    deal,
                    id: None,
                });
            }
        }
    }
    Ok(report)
}

/// Whether an already-taken lead is the one a duplicate row matched — the
/// question that tells "already in CRM" from "twice in this file".
fn matches_lead(taken: &str, address: &str, reason: DuplicateReason) -> bool {
    let taken = taken.to_ascii_lowercase();
    let address = address.to_ascii_lowercase();
    match reason {
        DuplicateReason::Email => taken == address,
        DuplicateReason::Domain => match (domain_of(&taken), domain_of(&address)) {
            (Some(a), Some(b)) => a == b,
            _ => false,
        },
    }
}

/// One row as a deal, or the rule it broke.
fn lead_from_row(row: &CsvRow, columns: &Columns) -> Result<NewDeal> {
    let company = bounded(
        "company name",
        row.field(columns.company),
        DEAL_PARTY_MAX_CHARS,
    )?;
    let stated_title = bounded("title", row.field(columns.title), DEAL_TITLE_MAX_CHARS)?;
    // A lead is named after the opportunity when the file says so, and after
    // the company when it does not — a file of companies is the commonest
    // lead list there is, and refusing it for having no "title" column would
    // be a rule invented by an importer.
    let title = if stated_title.is_empty() {
        company.clone()
    } else {
        stated_title
    };
    let title = required("title", &title, DEAL_TITLE_MAX_CHARS).map_err(|_| {
        StoreError::Validation("the row states neither a title nor a company".to_owned())
    })?;
    let email = parse_email(row.field(columns.email))?;
    let currency_code = match row.field(columns.currency) {
        "" => DEFAULT_CURRENCY.to_owned(),
        stated => currency(stated)?,
    };
    Ok(NewDeal {
        title,
        customer_id: None,
        contact_id: None,
        company_name: company,
        contact_name: bounded(
            "contact name",
            row.field(columns.contact_name),
            DEAL_PARTY_MAX_CHARS,
        )?,
        contact_email: email,
        value_cents: parse_value_cents(row.field(columns.value))?,
        currency: currency_code,
        expected_close: parse_day(row.field(columns.expected_close))?,
        // Never the file's: the importing user owns what they import, and a
        // user id in a spreadsheet cell is not an owner.
        owner_user_id: None,
        source: bounded("source", row.field(columns.source), DEAL_SOURCE_MAX_CHARS)?,
    })
}

/// An address, or nothing. A cell that is not an address at all is a refusal
/// and never a stored non-address: every duplicate rule in this module turns
/// on it being one.
fn parse_email(raw: &str) -> Result<String> {
    let value = bounded("contact email", raw, DEAL_EMAIL_MAX_CHARS)?;
    if value.is_empty() {
        return Ok(value);
    }
    if domain_of(&value).is_none() {
        return Err(StoreError::Validation(
            "the email column does not hold an email address".to_owned(),
        ));
    }
    Ok(value)
}

/// A day, or nothing. ISO 8601 only: `03/04/2026` is the third of April in
/// Paris and the fourth of March in New York, and an expected close date read
/// the wrong way round is a forecast that is silently wrong.
fn parse_day(raw: &str) -> Result<Option<Date>> {
    if raw.is_empty() {
        return Ok(None);
    }
    Date::parse(raw, &Iso8601::DATE).map(Some).map_err(|_| {
        StoreError::Validation("the expected close date must be written YYYY-MM-DD".to_owned())
    })
}

/// An amount in the deal's currency, as integer cents.
///
/// The grammar — every way Europe writes an amount, and the one shape
/// (`1.234`) that is refused rather than guessed — lives in
/// [`crate::money_text`], shared with the receipt extractor so the two cannot
/// come to different answers about the same characters. What this function
/// adds is the *column's* rules and the sentences a person importing a
/// spreadsheet should read:
///
/// - an empty cell is an **unpriced lead**, worth nothing rather than wrong;
/// - a negative value is not a discount, it is a typo;
/// - the deal ceiling ([`DEAL_VALUE_MAX_CENTS`]) applies here too, so a stray
///   column of phone numbers cannot import as a billion-euro pipeline.
///
/// # Errors
///
/// [`StoreError::Validation`] naming the rule the cell broke.
pub fn parse_value_cents(raw: &str) -> Result<i64> {
    let too_large = || {
        StoreError::Validation(format!(
            "a deal value must be between 0 and {DEAL_VALUE_MAX_CENTS} cents"
        ))
    };
    let total = match parse_amount_cents(raw) {
        Ok(total) => total,
        Err(AmountText::Empty) => return Ok(0),
        Err(AmountText::Negative) => {
            return Err(StoreError::Validation(
                "a deal value must not be negative".to_owned(),
            ));
        }
        Err(AmountText::Ambiguous) => {
            return Err(StoreError::Validation(
                "a value like 1.234 is a thousand in one country and one and a bit in another; \
                 write it with two decimals, or with no separator at all"
                    .to_owned(),
            ));
        }
        Err(AmountText::Grouping) => {
            return Err(StoreError::Validation(
                "the value column's thousands separators are not in groups of three".to_owned(),
            ));
        }
        Err(AmountText::NotANumber) => {
            return Err(StoreError::Validation(
                "the value column does not hold an amount".to_owned(),
            ));
        }
        Err(AmountText::TooLarge) => return Err(too_large()),
    };
    if total > DEAL_VALUE_MAX_CENTS {
        return Err(too_large());
    }
    Ok(total)
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use crate::csv_read::parse as parse_csv;

    fn table(text: &str) -> CsvTable {
        parse_csv(text.as_bytes(), MAX_IMPORT_ROWS).expect("readable CSV")
    }

    fn cents(raw: &str) -> i64 {
        parse_value_cents(raw).unwrap_or_else(|e| panic!("{raw:?} refused: {e:?}"))
    }

    fn refused(raw: &str) -> String {
        match parse_value_cents(raw) {
            Err(StoreError::Validation(rule)) => rule,
            other => panic!("{raw:?} was accepted: {other:?}"),
        }
    }

    #[test]
    fn every_way_europe_writes_an_amount_reads_as_the_same_cents() {
        assert_eq!(cents("1234"), 123_400);
        assert_eq!(cents("1234.56"), 123_456);
        assert_eq!(cents("1234,56"), 123_456);
        assert_eq!(cents("1.234,56"), 123_456);
        assert_eq!(cents("1,234.56"), 123_456);
        assert_eq!(cents("1.234.567"), 123_456_700);
        assert_eq!(cents("1 234 567"), 123_456_700);
        assert_eq!(cents("€ 1 234,50"), 123_450);
        assert_eq!(cents("1234,5"), 123_450, "one decimal digit is tenths");
        assert_eq!(cents(""), 0, "an unpriced lead is worth nothing, not wrong");
        assert_eq!(cents("0"), 0);
    }

    #[test]
    fn the_one_ambiguous_shape_is_refused_rather_than_guessed() {
        let rule = refused("1.234");
        assert!(rule.contains("one country"), "{rule}");
        assert!(refused("1,234").contains("one country"));
        // Grouped, so no longer ambiguous.
        assert_eq!(cents("1.234.000"), 123_400_000);
    }

    #[test]
    fn a_value_that_is_not_an_amount_is_refused() {
        for bad in ["abc", "12abc", "1.2.3", "12,3456", "1.23,45,6", "--1"] {
            let rule = refused(bad);
            assert!(!rule.is_empty(), "{bad}");
        }
        assert!(refused("-5").contains("negative"));
        assert!(refused("1234,5678").contains("does not hold an amount"));
        assert!(refused("12.34.567").contains("groups of three"));
    }

    #[test]
    fn a_value_above_the_deal_ceiling_is_refused_and_never_wraps() {
        assert!(refused("9999999999999").contains("cents"));
        assert_eq!(cents("1000000000"), DEAL_VALUE_MAX_CENTS);
    }

    #[test]
    fn a_day_is_read_only_in_the_form_that_means_one_thing() {
        assert_eq!(
            parse_day("2026-09-30").unwrap(),
            Some(Date::from_calendar_date(2026, time::Month::September, 30).unwrap())
        );
        assert_eq!(parse_day("").unwrap(), None);
        for bad in ["30/09/2026", "09/30/2026", "30.09.2026", "next tuesday"] {
            assert!(parse_day(bad).is_err(), "{bad} was accepted");
        }
    }

    #[test]
    fn an_email_column_that_does_not_hold_an_email_is_refused() {
        assert_eq!(parse_email("ada@acme.example").unwrap(), "ada@acme.example");
        assert_eq!(parse_email("  ").unwrap(), "");
        for bad in ["ada", "ada@", "@acme.example", "ada@acme", "a@b@c.example"] {
            assert!(parse_email(bad).is_err(), "{bad} was accepted");
        }
    }

    #[test]
    fn the_header_is_guessed_in_the_three_languages_we_ship() {
        let mapping = LeadMapping::infer(&table(
            "Company name,Contact,E-Mail,Amount,Currency,Expected close,Source\n",
        ));
        assert_eq!(mapping.company.as_deref(), Some("Company name"));
        assert_eq!(mapping.email.as_deref(), Some("E-Mail"));
        assert_eq!(mapping.value.as_deref(), Some("Amount"));
        assert_eq!(mapping.expected_close.as_deref(), Some("Expected close"));
        assert_eq!(mapping.title, None, "no column means no guess");

        let dutch = LeadMapping::infer(&table("Bedrijf;Naam;Email;Bedrag\n"));
        assert_eq!(dutch.company.as_deref(), Some("Bedrijf"));
        assert_eq!(dutch.contact_name.as_deref(), Some("Naam"));
        assert_eq!(dutch.value.as_deref(), Some("Bedrag"));

        let french = LeadMapping::infer(&table("Société;Contact;Courriel;Montant\n"));
        assert_eq!(french.company.as_deref(), Some("Société"));
        assert_eq!(french.email.as_deref(), Some("Courriel"));
        assert_eq!(french.value.as_deref(), Some("Montant"));
    }

    #[test]
    fn a_mapping_naming_a_column_the_file_lacks_is_refused() {
        let table = table("Company,Email\nAcme,ada@acme.example\n");
        let mapping = LeadMapping {
            company: Some("Company".to_owned()),
            value: Some("Turnover".to_owned()),
            ..LeadMapping::default()
        };
        match classify(&table, &mapping, Known::default()) {
            Err(StoreError::Validation(rule)) => {
                assert!(rule.contains("no column mapped to value"), "{rule}");
            }
            other => panic!("expected a refusal, got {other:?}"),
        }
    }

    #[test]
    fn a_file_nothing_can_be_read_from_is_one_refusal_not_a_page_of_them() {
        // A German export this product does not guess the words of.
        let table = table("Firma;Ansprechpartner;Umsatz\nAcme GmbH;Ada;900\n");
        match classify(&table, &LeadMapping::default(), Known::default()) {
            Err(StoreError::Validation(rule)) => {
                assert!(rule.contains("title or a company name"), "{rule}");
            }
            other => panic!("expected the mapping refusal, got {other:?}"),
        }
        // Told which column holds the company, the same file reads.
        let mapping = LeadMapping {
            company: Some("Firma".to_owned()),
            ..LeadMapping::default()
        };
        let report = classify(&table, &mapping, Known::default()).unwrap();
        assert_eq!(report.leads.len(), 1);
    }

    #[test]
    fn a_plain_file_becomes_leads_with_the_guessed_mapping() {
        let table = table(
            "Company;Contact;E-mail;Amount;Expected close\r\n\
             Acme GmbH;Ada;ada@acme.example;12.500,00;2026-09-30\r\n\
             Beta BV;Bob;bob@beta.example;900;\r\n",
        );
        let report = classify(&table, &LeadMapping::default(), Known::default()).unwrap();
        assert_eq!(report.total_rows, 2);
        assert_eq!(report.leads.len(), 2);
        assert!(report.duplicates.is_empty() && report.errors.is_empty());
        assert!(!report.committed, "classification writes nothing");
        assert_eq!(report.delimiter, ';');
        let first = &report.leads[0];
        assert_eq!(first.line, 2);
        assert_eq!(first.deal.title, "Acme GmbH", "the company names the lead");
        assert_eq!(first.deal.company_name, "Acme GmbH");
        assert_eq!(first.deal.contact_name, "Ada");
        assert_eq!(first.deal.value_cents, 1_250_000);
        assert_eq!(first.deal.currency, DEFAULT_CURRENCY);
        assert!(first.deal.expected_close.is_some());
        assert_eq!(report.leads[1].deal.value_cents, 90_000);
        assert_eq!(report.leads[1].deal.expected_close, None);
    }

    #[test]
    fn a_title_column_names_the_lead_and_the_company_stays_the_company() {
        let table = table("Opportunity,Company,Email\nRenewal,Acme GmbH,ada@acme.example\n");
        let report = classify(&table, &LeadMapping::default(), Known::default()).unwrap();
        assert_eq!(report.leads[0].deal.title, "Renewal");
        assert_eq!(report.leads[0].deal.company_name, "Acme GmbH");
    }

    #[test]
    fn a_row_with_neither_a_title_nor_a_company_is_the_only_row_that_fails() {
        let table = table(
            "Company,Email\n\
             ,ada@acme.example\n\
             Beta BV,bob@beta.example\n",
        );
        let report = classify(&table, &LeadMapping::default(), Known::default()).unwrap();
        assert_eq!(report.errors.len(), 1);
        assert_eq!(report.errors[0].line, 2);
        assert!(
            report.errors[0]
                .rule
                .contains("neither a title nor a company")
        );
        assert_eq!(report.leads.len(), 1, "the good row still stands");
        assert_eq!(report.leads[0].line, 3);
    }

    #[test]
    fn the_rule_a_row_broke_never_quotes_the_row() {
        let table = table("Company,Email,Amount\nAcme,not-an-address,10\n");
        let report = classify(&table, &LeadMapping::default(), Known::default()).unwrap();
        assert_eq!(report.errors.len(), 1);
        assert!(!report.errors[0].rule.contains("not-an-address"));
        assert!(!report.errors[0].rule.contains("Acme"));
    }

    #[test]
    fn an_address_the_tenant_already_knows_is_skipped_and_reported() {
        let mut known = Known::default();
        known.remember("Ada@Acme.example");
        let table = table(
            "Company,Email\n\
             Acme GmbH,ada@acme.example\n\
             Gamma SA,gamma@gamma.example\n",
        );
        let report = classify(&table, &LeadMapping::default(), known).unwrap();
        assert_eq!(report.duplicates.len(), 1);
        assert_eq!(report.duplicates[0].line, 2);
        assert_eq!(report.duplicates[0].reason, DuplicateReason::Email);
        assert_eq!(report.duplicates[0].source, DuplicateSource::Crm);
        assert_eq!(report.duplicates[0].matched, "ada@acme.example");
        assert_eq!(report.leads.len(), 1);
    }

    #[test]
    fn a_company_domain_the_tenant_already_deals_with_is_a_duplicate() {
        let mut known = Known::default();
        known.remember("bob@acme.example");
        let table = table("Company,Email\nAcme GmbH,ada@acme.example\n");
        let report = classify(&table, &LeadMapping::default(), known).unwrap();
        assert_eq!(report.duplicates.len(), 1);
        assert_eq!(report.duplicates[0].reason, DuplicateReason::Domain);
        assert_eq!(report.duplicates[0].matched, "acme.example");
        assert!(report.leads.is_empty());
    }

    #[test]
    fn a_free_mail_domain_never_folds_two_unrelated_people_into_one() {
        let mut known = Known::default();
        known.remember("someone@gmail.com");
        let unrelated = table("Company,Email\nAda's bakery,ada@gmail.com\n");
        let report = classify(&unrelated, &LeadMapping::default(), known).unwrap();
        assert!(report.duplicates.is_empty(), "{:?}", report.duplicates);
        assert_eq!(report.leads.len(), 1);
        // The same address, though, is still the same person.
        let same = table("Company,Email\nAda's bakery,someone@gmail.com\n");
        let mut known = Known::default();
        known.remember("someone@gmail.com");
        let report = classify(&same, &LeadMapping::default(), known).unwrap();
        assert_eq!(report.duplicates.len(), 1);
        assert_eq!(report.duplicates[0].reason, DuplicateReason::Email);
    }

    #[test]
    fn one_company_listed_twice_in_a_file_is_imported_once() {
        let table = table(
            "Company,Contact,Email\n\
             Acme GmbH,Ada,ada@acme.example\n\
             Acme GmbH,Bob,bob@acme.example\n\
             Acme GmbH,Ada again,ada@acme.example\n",
        );
        let report = classify(&table, &LeadMapping::default(), Known::default()).unwrap();
        assert_eq!(report.leads.len(), 1);
        assert_eq!(report.duplicates.len(), 2);
        assert_eq!(report.duplicates[0].reason, DuplicateReason::Domain);
        assert_eq!(report.duplicates[0].source, DuplicateSource::File);
        assert_eq!(report.duplicates[1].reason, DuplicateReason::Email);
        assert_eq!(report.duplicates[1].source, DuplicateSource::File);
    }

    #[test]
    fn a_row_with_no_address_is_never_a_duplicate_of_another_one() {
        let table = table(
            "Company,Email\n\
             Acme GmbH,\n\
             Beta BV,\n",
        );
        let report = classify(&table, &LeadMapping::default(), Known::default()).unwrap();
        assert_eq!(report.leads.len(), 2, "{:?}", report.duplicates);
    }

    #[test]
    fn an_explicit_mapping_beats_the_guess() {
        let table = table("Firma,Verkäufer,Kontakt,Umsatz\nAcme GmbH,Ida,ada@acme.example,900\n");
        let mapping = LeadMapping {
            company: Some("Firma".to_owned()),
            email: Some("Kontakt".to_owned()),
            value: Some("Umsatz".to_owned()),
            contact_name: Some("Verkäufer".to_owned()),
            ..LeadMapping::default()
        };
        let report = classify(&table, &mapping, Known::default()).unwrap();
        assert_eq!(report.leads.len(), 1);
        let deal = &report.leads[0].deal;
        assert_eq!(deal.company_name, "Acme GmbH");
        assert_eq!(deal.contact_email, "ada@acme.example");
        assert_eq!(deal.contact_name, "Ida");
        assert_eq!(deal.value_cents, 90_000);
        assert_eq!(report.mapping, mapping, "the report says what it used");
    }

    #[test]
    fn a_currency_column_is_honoured_and_a_bad_one_fails_only_its_row() {
        let table = table(
            "Company,Amount,Currency\n\
             Acme GmbH,100,usd\n\
             Beta BV,100,euros\n",
        );
        let report = classify(&table, &LeadMapping::default(), Known::default()).unwrap();
        assert_eq!(report.leads.len(), 1);
        assert_eq!(report.leads[0].deal.currency, "USD", "uppercased once");
        assert_eq!(report.errors.len(), 1);
        assert!(report.errors[0].rule.contains("ISO 4217"));
    }

    #[test]
    fn an_over_long_field_fails_its_row_and_not_the_file() {
        let long = "x".repeat(DEAL_TITLE_MAX_CHARS + 1);
        let table = table(&format!(
            "Company,Email\n{long},ada@acme.example\nBeta BV,b@b.example\n"
        ));
        let report = classify(&table, &LeadMapping::default(), Known::default()).unwrap();
        assert_eq!(report.errors.len(), 1);
        assert!(report.errors[0].rule.contains("at most"));
        assert_eq!(report.leads.len(), 1);
    }
}
