//! A bank's CSV export, plus the mapping a person confirmed (alo Finance, ADR
//! 0035, wave B4.08c; `docs/design/finance.md`, "The bank and reconciliation").
//!
//! CAMT.053 and MT940 are specifications: a file either is one or is not, and
//! the parser decides alone. A CSV export is not a format at all — it is
//! whatever a bank's web portal felt like writing that year — so this reader
//! cannot decide alone and does not try. It reads what a **person confirmed**
//! in the wizard: which column is the date, which holds the amount, and the two
//! conventions no file states about itself.
//!
//! # The two conventions, and why they are asked rather than guessed
//!
//! - **`03/04/2026` is two different days.** Day-first in Paris, month-first in
//!   New York, and a bank statement whose dates are three weeks out reconciles
//!   against the wrong invoices. So the order is *inferred from the file as a
//!   whole* — a single row with a day past the twelfth settles it for every
//!   row, and a dot separator settles it outright, because no month-first
//!   locale writes `03.04.2026` — and when the whole column stays ambiguous the
//!   file is **refused** with the words that tell a person to state the order.
//! - **`1.234` is a thousand or it is one and a bit** ([`crate::money_text`],
//!   whose refusal this reader inherits). Stating the decimal convention makes
//!   it exact; leaving it to be guessed makes it a factor of a thousand.
//!
//! Neither is a preference to be remembered per tenant yet: the mapping travels
//! with the upload, the preview shows what it produced, and a saved mapping is
//! its own item once real files have shown which ones repeat.
//!
//! # Three shapes of money, one signed integer
//!
//! Banks state direction in three ways, and this reader takes all three because
//! all three are in the wild: **one signed column** (`-120,00`, or the German
//! trailing minus `120,00-`, or `(120,00)`), a **debit and a credit column**
//! where exactly one is filled, or an **amount plus a sign column** (`S`/`H`,
//! `D`/`C`, `Af`/`Bij`). What comes out is [`ParsedLine::amount_cents`]: signed
//! integer cents, positive is money in, decided once here so that nothing
//! downstream re-decides which way a number points.
//!
//! # A row that cannot be read stops the file
//!
//! Nothing is imported halfway. A row this reader cannot turn into a
//! transaction becomes a [`RowError`] naming its line and the rule — never its
//! content, which is the tenant's own money (Law 1) — and the import writes
//! nothing at all. The preview is what makes that cheap: a person sees every
//! broken row before committing, fixes the file once, and uploads once.
//!
//! The one row that is skipped rather than refused is the row that is **blank
//! in every mapped column**: a trailing separator line is not a transaction and
//! is not a mistake either. It is still counted and reported, because a person
//! who is told "11 of 12 rows" must be able to find the twelfth.

use time::{Date, Month};

use crate::bank_import::{BankSource, LINE_AMOUNT_MAX_CENTS, ParsedLine, ParsedStatement};
use crate::bank_read::BankImportRequest;
use crate::billing_field::{DEFAULT_CURRENCY, currency as currency_code};
use crate::csv_read::{CsvRow, CsvTable, RowError};
use crate::error::{Result, StoreError};
use crate::money_text::{AmountText, parse_amount_cents, strip_decoration};

/// How this file writes a date, when the file itself cannot settle it.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum BankCsvDates {
    /// Read it from the file: a four-digit first component is a year, a dot
    /// separator is European, a component past twelve is a day. Refused when
    /// the whole column stays ambiguous.
    #[default]
    Auto,
    /// Day first — `03/04/2026` is the third of April.
    Dmy,
    /// Month first — `03/04/2026` is the fourth of March.
    Mdy,
    /// Year first — `2026-04-03`.
    Ymd,
}

impl BankCsvDates {
    /// The word this order is stated and reported as.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::Dmy => "dmy",
            Self::Mdy => "mdy",
            Self::Ymd => "ymd",
        }
    }

    /// The order a stated word names, or `None` when it is not one of ours.
    #[must_use]
    pub fn parse(raw: &str) -> Option<Self> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "auto" | "" => Some(Self::Auto),
            "dmy" => Some(Self::Dmy),
            "mdy" => Some(Self::Mdy),
            "ymd" => Some(Self::Ymd),
            _ => None,
        }
    }
}

/// How this file writes the decimal separator.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum BankCsvDecimal {
    /// Read each amount on its own terms, refusing the one shape that is a
    /// factor of a thousand either way (`1.234`).
    #[default]
    Auto,
    /// A comma — `1.234,56`.
    Comma,
    /// A dot — `1,234.56`.
    Dot,
}

impl BankCsvDecimal {
    /// The word this convention is stated and reported as.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::Comma => "comma",
            Self::Dot => "dot",
        }
    }

    /// The convention a stated word names, or `None` when it is not one of ours.
    #[must_use]
    pub fn parse(raw: &str) -> Option<Self> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "auto" | "" => Some(Self::Auto),
            "comma" => Some(Self::Comma),
            "dot" | "point" => Some(Self::Dot),
            _ => None,
        }
    }
}

/// Which column of the file carries which part of a transaction.
///
/// Every field is a **column name** as it appears in the header, matched case-
/// and space-insensitively ([`CsvTable::column`]), and every one is optional
/// here — what is actually required is checked once the mapping meets a file,
/// so that the refusal can name the columns that file has. The caller states
/// the mapping; [`BankCsvMapping::infer`] is the first guess a wizard shows for
/// a person to correct.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct BankCsvMapping {
    /// The day the bank booked it — the day the books use. Required.
    pub booked_on: Option<String>,
    /// The day it takes value from; the booking date when unmapped.
    pub value_on: Option<String>,
    /// One signed amount.
    pub amount: Option<String>,
    /// Money out, as a positive number in its own column.
    pub debit: Option<String>,
    /// Money in, as a positive number in its own column.
    pub credit: Option<String>,
    /// The column that says which way an unsigned amount points.
    pub sign: Option<String>,
    /// The line's own currency; the statement's when unmapped.
    pub currency: Option<String>,
    /// Who the money came from, or went to.
    pub counterparty_name: Option<String>,
    /// Their account.
    pub counterparty_iban: Option<String>,
    /// What was written on the payment.
    pub remittance: Option<String>,
    /// The bank's own reference for the entry.
    pub bank_ref: Option<String>,
}

/// The header names each field is guessed from, in the four languages a
/// European bank portal writes its exports in. Folded exactly as
/// [`CsvTable::column`] folds a header, so `Buchungs-Tag` and `buchungstag` are
/// one word, and ordered most specific first: `date` must not win over
/// `Buchungstag` in a file that has both.
const GUESSES: [(&str, &[&str]); 11] = [
    (
        "booked",
        &[
            "bookingdate",
            "buchungstag",
            "buchungsdatum",
            "transactiondate",
            "boekingsdatum",
            "transactiedatum",
            "dateoperation",
            "dateopération",
            "datedelopération",
            "date",
            "datum",
            "booked",
        ],
    ),
    (
        "value",
        &[
            "valuedate",
            "valutadatum",
            "wertstellung",
            "wertstellungstag",
            "datevaleur",
            "rentedatum",
            "valuta date",
        ],
    ),
    (
        "amount",
        &[
            "amount",
            "betrag",
            "bedrag",
            "montant",
            "transactionamount",
            "amounteur",
            "somme",
        ],
    ),
    (
        "debit",
        &[
            "debit",
            "débit",
            "soll",
            "belastung",
            "uitgaven",
            "withdrawal",
            "paidout",
        ],
    ),
    (
        "credit",
        &[
            "credit",
            "crédit",
            "haben",
            "gutschrift",
            "inkomsten",
            "deposit",
            "paidin",
        ],
    ),
    (
        "sign",
        &[
            "debit/credit",
            "debitcredit",
            "soll/haben",
            "sollhaben",
            "af/bij",
            "afbij",
            "dcindicator",
            "vorzeichen",
            "sens",
            "d/c",
            "dc",
            "sign",
        ],
    ),
    (
        "currency",
        &[
            "currency", "währung", "waehrung", "devise", "valuta", "munt",
        ],
    ),
    (
        "counterparty",
        &[
            "counterparty",
            "beneficiary",
            "payee",
            "zahlungsempfänger",
            "empfänger",
            "empfaenger",
            "begünstigter",
            "auftraggeber",
            "naamtegenrekening",
            "tegenrekeninghouder",
            "bénéficiaire",
            "nomcontrepartie",
            "name",
            "naam",
            "nom",
        ],
    ),
    (
        "iban",
        &[
            "counterpartyiban",
            "ibantegenrekening",
            "tegenrekening",
            "empfängeriban",
            "comptebénéficiaire",
            "contrepartieiban",
            "iban",
        ],
    ),
    (
        "remittance",
        &[
            "verwendungszweck",
            "remittance",
            "remittanceinformation",
            "omschrijving",
            "mededelingen",
            "communication",
            "libellé",
            "paymentdetails",
            "description",
            "betreff",
            "memo",
        ],
    ),
    (
        "reference",
        &[
            "bankreference",
            "transactionreference",
            "kundenreferenz",
            "endtoendid",
            "transactionid",
            "referentie",
            "référence",
            "reference",
        ],
    ),
];

impl BankCsvMapping {
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
            booked_on: pick("booked"),
            value_on: pick("value"),
            amount: pick("amount"),
            debit: pick("debit"),
            credit: pick("credit"),
            sign: pick("sign"),
            currency: pick("currency"),
            counterparty_name: pick("counterparty"),
            counterparty_iban: pick("iban"),
            remittance: pick("remittance"),
            bank_ref: pick("reference"),
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
    /// stage a month of blank transactions.
    fn resolve(&self, table: &CsvTable) -> Result<Columns> {
        let at = |field: &str, name: &Option<String>| match name {
            None => Ok(None),
            Some(name) => table.column(name).map(Some).ok_or_else(|| {
                StoreError::Validation(format!("the file has no column mapped to {field}"))
            }),
        };
        let columns = Columns {
            booked_on: at("the booking date", &self.booked_on)?,
            value_on: at("the value date", &self.value_on)?,
            amount: at("the amount", &self.amount)?,
            debit: at("the debit amount", &self.debit)?,
            credit: at("the credit amount", &self.credit)?,
            sign: at("the debit/credit indicator", &self.sign)?,
            currency: at("the currency", &self.currency)?,
            counterparty_name: at("the counterparty", &self.counterparty_name)?,
            counterparty_iban: at("the counterparty's account", &self.counterparty_iban)?,
            remittance: at("the payment reference", &self.remittance)?,
            bank_ref: at("the bank's reference", &self.bank_ref)?,
        };
        if columns.booked_on.is_none() {
            return Err(StoreError::Validation(
                "the mapping does not say which column holds the booking date, and a transaction \
                 without a day cannot be reconciled"
                    .to_owned(),
            ));
        }
        if columns.amount.is_none() && columns.debit.is_none() && columns.credit.is_none() {
            return Err(StoreError::Validation(
                "the mapping does not say which column holds the amount: map one signed amount, \
                 or a debit and a credit column"
                    .to_owned(),
            ));
        }
        Ok(columns)
    }
}

/// The mapping as column indices.
#[derive(Debug, Clone, Copy, Default)]
struct Columns {
    booked_on: Option<usize>,
    value_on: Option<usize>,
    amount: Option<usize>,
    debit: Option<usize>,
    credit: Option<usize>,
    sign: Option<usize>,
    currency: Option<usize>,
    counterparty_name: Option<usize>,
    counterparty_iban: Option<usize>,
    remittance: Option<usize>,
    bank_ref: Option<usize>,
}

impl Columns {
    /// Every column this mapping actually points at.
    fn all(self) -> [Option<usize>; 11] {
        [
            self.booked_on,
            self.value_on,
            self.amount,
            self.debit,
            self.credit,
            self.sign,
            self.currency,
            self.counterparty_name,
            self.counterparty_iban,
            self.remittance,
            self.bank_ref,
        ]
    }
}

/// What a mapped file said, before anything is written.
///
/// Both a preview and a commit produce one of these, from the same call — so a
/// preview cannot promise a statement the commit then reads differently.
#[derive(Debug, Clone)]
pub struct CsvReading {
    /// The mapping actually used: the caller's, or the header's own guess.
    pub mapping: BankCsvMapping,
    /// The date order actually used — never [`BankCsvDates::Auto`] once a file
    /// has been read, because a file that could not settle it is refused.
    pub dates: BankCsvDates,
    /// The decimal convention used, as stated.
    pub decimal: BankCsvDecimal,
    /// The file line each transaction came from, parallel to
    /// [`ParsedStatement::lines`].
    pub at: Vec<usize>,
    /// The lines that were blank in every mapped column, and therefore not
    /// transactions.
    pub skipped: Vec<usize>,
    /// The rows that cannot be read, in file order. One of these means the
    /// import writes nothing.
    pub errors: Vec<RowError>,
    /// The statement these rows make, or `None` when a row could not be read.
    pub statement: Option<ParsedStatement>,
}

/// Reads a mapped CSV export as a statement.
///
/// # Errors
/// [`StoreError::Validation`] when the mapping names a column the file has not
/// got, when it is missing a date or an amount, when the file's date order
/// cannot be settled, or when the file holds no transactions at all. A row that
/// is merely unreadable is a [`RowError`] in the reading rather than an error
/// here: the report is the answer, and it names every broken line at once.
pub fn read_csv_statement(table: &CsvTable, request: &BankImportRequest) -> Result<CsvReading> {
    let mapping = if request.mapping.is_empty() {
        BankCsvMapping::infer(table)
    } else {
        request.mapping.clone()
    };
    let columns = mapping.resolve(table)?;
    let currency = statement_currency(request)?;
    let dates = match request.dates {
        BankCsvDates::Auto => infer_order(table, columns)?,
        stated => stated,
    };

    let mut lines: Vec<ParsedLine> = Vec::new();
    let mut at: Vec<usize> = Vec::new();
    let mut skipped: Vec<usize> = Vec::new();
    let mut errors: Vec<RowError> = Vec::new();
    for row in &table.rows {
        if columns
            .all()
            .into_iter()
            .all(|column| row.field(column).is_empty())
        {
            skipped.push(row.line);
            continue;
        }
        match read_row(row, columns, dates, request.decimal, &currency) {
            Ok(line) => {
                lines.push(line);
                at.push(row.line);
            }
            Err(rule) => errors.push(RowError {
                line: row.line,
                rule,
            }),
        }
    }

    if errors.is_empty() && lines.is_empty() {
        return Err(StoreError::Validation(
            "this file states no transactions: every row was blank in the mapped columns"
                .to_owned(),
        ));
    }
    // A file with one broken row stages none of it, so the statement is not
    // built at all — there is nothing for a caller to write by mistake.
    let statement = if errors.is_empty() {
        period(&lines).map(|(from_date, to_date)| ParsedStatement {
            source: BankSource::Csv,
            account_iban: request.account_iban.clone(),
            currency,
            // A CSV export has no name of its own; the other two formats state
            // one.
            statement_ref: String::new(),
            // And it states no balances, which is absent rather than zero: a
            // zero would be a reconciliation target that quietly disagrees
            // with reality.
            opening_balance_cents: None,
            closing_balance_cents: None,
            from_date,
            to_date,
            lines,
            unbooked: 0,
        })
    } else {
        None
    };
    Ok(CsvReading {
        mapping,
        dates,
        decimal: request.decimal,
        at,
        skipped,
        errors,
        statement,
    })
}

/// The statement's currency: the one the caller stated, or the tenant's
/// default. A CSV export states none of its own.
///
/// # Errors
/// [`StoreError::Validation`] when the stated code is not ISO 4217.
fn statement_currency(request: &BankImportRequest) -> Result<String> {
    match request.currency.as_deref().map(str::trim) {
        None | Some("") => Ok(DEFAULT_CURRENCY.to_owned()),
        Some(stated) => currency_code(stated),
    }
}

/// The period the transactions cover, or `None` when there are none.
fn period(lines: &[ParsedLine]) -> Option<(Date, Date)> {
    let first = lines.first()?.booked_on;
    Some(lines.iter().fold((first, first), |(from, to), line| {
        (from.min(line.booked_on), to.max(line.booked_on))
    }))
}

/// One row as a transaction, or the rule it broke.
fn read_row(
    row: &CsvRow,
    columns: Columns,
    dates: BankCsvDates,
    decimal: BankCsvDecimal,
    statement_currency: &str,
) -> std::result::Result<ParsedLine, String> {
    let booked_on = read_date(row.field(columns.booked_on), dates)
        .map_err(|reason| format!("the row's booking date {reason}"))?;
    let value_on = match row.field(columns.value_on) {
        "" => booked_on,
        raw => read_date(raw, dates).map_err(|reason| format!("the row's value date {reason}"))?,
    };
    let amount_cents = read_money(row, columns, decimal)?;
    if amount_cents == 0 {
        return Err(
            "the row moves nothing, and a transaction of zero is not one this reader can \
             reconcile"
                .to_owned(),
        );
    }
    if amount_cents.abs() > LINE_AMOUNT_MAX_CENTS {
        return Err("the row states an amount too large to be one".to_owned());
    }
    let currency = match row.field(columns.currency) {
        "" => statement_currency.to_owned(),
        raw => currency_code(raw)
            .map_err(|_| "the row's currency is not a three-letter ISO 4217 code".to_owned())?,
    };
    Ok(ParsedLine {
        booked_on,
        value_on,
        amount_cents,
        currency,
        // Length and IBAN validity are settled once, for all three formats, by
        // the staging step: a name one character over ISO's limit is clipped,
        // and an unreadable counterparty account is one blank field rather than
        // a lost month.
        counterparty_name: row.field(columns.counterparty_name).to_owned(),
        counterparty_iban: row.field(columns.counterparty_iban).to_owned(),
        remittance: row.field(columns.remittance).to_owned(),
        bank_ref: row.field(columns.bank_ref).to_owned(),
    })
}

/// The row's signed cents, from whichever of the three shapes the mapping
/// describes.
fn read_money(
    row: &CsvRow,
    columns: Columns,
    decimal: BankCsvDecimal,
) -> std::result::Result<i64, String> {
    let debit = row.field(columns.debit);
    let credit = row.field(columns.credit);
    if !debit.is_empty() || !credit.is_empty() {
        let debit_cents =
            read_cents(debit, decimal).map_err(|reason| format!("the row's debit {reason}"))?;
        let credit_cents =
            read_cents(credit, decimal).map_err(|reason| format!("the row's credit {reason}"))?;
        return match (debit_cents, credit_cents) {
            (Some(out), Some(into)) if out != 0 && into != 0 => Err(
                "the row states both a debit and a credit, and one transaction moves money one \
                 way"
                .to_owned(),
            ),
            // A column holding a signed number keeps its sign: some exports
            // write the reversal of a credit as a negative credit.
            (Some(out), _) if out != 0 => Ok(-out),
            (_, Some(into)) => Ok(into),
            (Some(out), None) => Ok(-out),
            (None, None) => Err("the row states no amount".to_owned()),
        };
    }
    let Some(cents) = read_cents(row.field(columns.amount), decimal)
        .map_err(|reason| format!("the row's amount {reason}"))?
    else {
        return Err("the row states no amount".to_owned());
    };
    match read_sign(row.field(columns.sign))? {
        // The sign column decides, and the amount contributes only its size:
        // an export that writes `-120,00` in a row marked `S` means one
        // hundred and twenty out, not one hundred and twenty in.
        Some(sign) => Ok(sign * cents.abs()),
        None => Ok(cents),
    }
}

/// Which way a sign column points: `1` in, `-1` out, `None` when it is blank or
/// unmapped.
fn read_sign(raw: &str) -> std::result::Result<Option<i64>, String> {
    if raw.is_empty() {
        return Ok(None);
    }
    let word = raw.trim().to_ascii_lowercase();
    let word = word.trim_matches(|c: char| !c.is_alphanumeric() && c != '+' && c != '-');
    match word {
        "c" | "cr" | "credit" | "crédit" | "h" | "haben" | "bij" | "+" | "gutschrift" | "in" => {
            Ok(Some(1))
        }
        "d" | "dr" | "debit" | "débit" | "s" | "soll" | "af" | "-" | "belastung" | "out" => {
            Ok(Some(-1))
        }
        _ => Err(
            "the row's debit/credit indicator is not one this reader knows (D/C, S/H, Af/Bij)"
                .to_owned(),
        ),
    }
}

/// One money cell as signed cents, or `None` when it is blank.
///
/// The sign lives here rather than in [`crate::money_text`], which reads the
/// grammar of a number and has no opinion about direction: a bank writes the
/// minus in front, behind (`120,00-`, which German software still does) or
/// around it (`(120,00)`), and all three mean the same thing.
fn read_cents(raw: &str, decimal: BankCsvDecimal) -> std::result::Result<Option<i64>, String> {
    // The currency symbol, the thin space a spreadsheet writes and the Swiss
    // apostrophe come off first — with the same list `money_text` uses, so a
    // stated convention reads exactly the decorations an unstated one does.
    let text = strip_decoration(raw);
    let text = text.trim();
    if text.is_empty() {
        return Ok(None);
    }
    let (negative, body) = strip_sign(text);
    let normalized = match decimal {
        BankCsvDecimal::Auto => body.to_owned(),
        BankCsvDecimal::Comma => normalize(body, ',', '.')?,
        BankCsvDecimal::Dot => normalize(body, '.', ',')?,
    };
    let cents = parse_amount_cents(&normalized).map_err(|reason| match reason {
        AmountText::Empty => "is blank where a number was expected".to_owned(),
        AmountText::Ambiguous => "could be a thousand or one and a bit: state whether this file \
                                  writes decimals with a comma or a dot"
            .to_owned(),
        AmountText::TooLarge => "is too large to be an amount".to_owned(),
        AmountText::Grouping | AmountText::NotANumber | AmountText::Negative => {
            "is not a number this reader can read exactly".to_owned()
        }
    })?;
    Ok(Some(if negative { -cents } else { cents }))
}

/// The sign a bank wrote in front of, behind or around the number, and the
/// number without it.
fn strip_sign(text: &str) -> (bool, &str) {
    if let Some(inner) = text.strip_prefix('(').and_then(|t| t.strip_suffix(')')) {
        return (true, inner.trim());
    }
    if let Some(rest) = text.strip_prefix('-') {
        return (true, rest.trim_start());
    }
    if let Some(rest) = text.strip_suffix('-') {
        return (true, rest.trim_end());
    }
    let rest = text.strip_prefix('+').unwrap_or(text);
    (false, rest.trim_start())
}

/// A number rewritten in the one convention [`parse_amount_cents`] cannot
/// misread: the grouping separator removed, the decimal separator a dot.
///
/// # Errors
/// The reason, when the stated convention leaves the cell unreadable — two
/// decimal separators, or more than two decimals.
fn normalize(body: &str, decimal: char, grouping: char) -> std::result::Result<String, String> {
    let without_grouping: String = body.chars().filter(|c| *c != grouping).collect();
    if without_grouping.matches(decimal).count() > 1 {
        return Err(
            "states its decimal separator twice, so it is not a number this reader can read \
             exactly"
                .to_owned(),
        );
    }
    let Some((units, cents)) = without_grouping.rsplit_once(decimal) else {
        return Ok(without_grouping);
    };
    if cents.chars().count() > 2 {
        return Err(
            "has more than two decimals, so it is not an amount in this currency".to_owned(),
        );
    }
    Ok(format!("{units}.{cents}"))
}

// ---- dates -------------------------------------------------------------------

/// A date split into its three components, with what the spelling itself says.
#[derive(Debug, Clone, Copy)]
struct DateParts {
    first: u32,
    second: u32,
    third: u32,
    /// The first component is four digits, so it is a year.
    first_wide: bool,
    /// The separator is a dot, which no month-first locale writes.
    dotted: bool,
}

/// The date order this file uses, inferred from every date in it.
///
/// One row is enough when it names a day past the twelfth, and a dot separator
/// is enough on its own. A file that disagrees with itself, or one that never
/// settles the question, is refused rather than read one of the two ways.
///
/// # Errors
/// [`StoreError::Validation`] naming what a person must state.
fn infer_order(table: &CsvTable, columns: Columns) -> Result<BankCsvDates> {
    let mut vote: Option<BankCsvDates> = None;
    let mut undecided = false;
    for row in &table.rows {
        for column in [columns.booked_on, columns.value_on] {
            let raw = row.field(column);
            if raw.is_empty() {
                continue;
            }
            let Some(parts) = split_date(raw) else {
                continue;
            };
            let Some(order) = order_of(parts) else {
                undecided = true;
                continue;
            };
            match vote {
                None => vote = Some(order),
                Some(seen) if seen == order => {}
                Some(_) => {
                    return Err(StoreError::Validation(
                        "this file's dates are not all written the same way round; state the date \
                         order, or export it again with ISO dates"
                            .to_owned(),
                    ));
                }
            }
        }
    }
    match (vote, undecided) {
        (Some(order), _) => Ok(order),
        (None, true) => Err(StoreError::Validation(
            "this file's dates could be day-first or month-first and nothing in it settles which; \
             state the date order"
                .to_owned(),
        )),
        // No date was readable anywhere. That is not a question about order,
        // and answering it here would hide the real one: every row says for
        // itself that its date is missing or is not a date.
        (None, false) => Ok(BankCsvDates::Ymd),
    }
}

/// What one spelling says about the order, when it says anything.
fn order_of(parts: DateParts) -> Option<BankCsvDates> {
    if parts.first_wide {
        return Some(BankCsvDates::Ymd);
    }
    if parts.dotted {
        return Some(BankCsvDates::Dmy);
    }
    if parts.first > 12 {
        return Some(BankCsvDates::Dmy);
    }
    if parts.second > 12 {
        return Some(BankCsvDates::Mdy);
    }
    None
}

/// A date cell as a day, or the reason it is not one.
fn read_date(raw: &str, dates: BankCsvDates) -> std::result::Result<Date, String> {
    if raw.is_empty() {
        return Err("is missing".to_owned());
    }
    let parts = split_date(raw).ok_or_else(|| "is not a date this reader can read".to_owned())?;
    let stated = match dates {
        BankCsvDates::Auto => order_of(parts).unwrap_or(BankCsvDates::Dmy),
        stated => stated,
    };
    // A spelling that names its own order wins over the file's: `2026-01-05` in
    // a day-first file is still the fifth of January, because a four-digit
    // first component cannot be a day. The reverse is refused rather than
    // guessed — in a file of ISO dates, `05-01-26` is a row nobody can read the
    // same way twice.
    let order = if parts.first_wide {
        BankCsvDates::Ymd
    } else if stated == BankCsvDates::Ymd {
        return Err("is not written the way the rest of this file's dates are".to_owned());
    } else {
        stated
    };
    let (year, month, day) = match order {
        BankCsvDates::Ymd => (parts.first, parts.second, parts.third),
        BankCsvDates::Mdy => (parts.third, parts.first, parts.second),
        // `Auto` cannot reach here: it was resolved above.
        BankCsvDates::Dmy | BankCsvDates::Auto => (parts.third, parts.second, parts.first),
    };
    build_date(year, month, day)
}

/// The three numbers of a date cell, whichever of the separators it uses.
fn split_date(raw: &str) -> Option<DateParts> {
    let text = raw.trim();
    // The compact spelling a portal writes when it means ISO: `20260105`.
    if text.len() == 8 && text.bytes().all(|b| b.is_ascii_digit()) {
        return Some(DateParts {
            first: text.get(0..4)?.parse().ok()?,
            second: text.get(4..6)?.parse().ok()?,
            third: text.get(6..8)?.parse().ok()?,
            first_wide: true,
            dotted: false,
        });
    }
    // A cell that carries a time as well (`05/01/2026 14:03`) is a date with a
    // clock attached, and the clock is not something a statement line keeps.
    let head = text.split_whitespace().next()?;
    let separator = ['-', '/', '.']
        .into_iter()
        .find(|candidate| head.contains(*candidate))?;
    if ['-', '/', '.']
        .into_iter()
        .any(|other| other != separator && head.contains(other))
    {
        return None;
    }
    let fields: Vec<&str> = head.split(separator).collect();
    let [first, second, third] = fields.as_slice() else {
        return None;
    };
    if [first, second, third]
        .iter()
        .any(|part| part.is_empty() || part.len() > 4 || !part.bytes().all(|b| b.is_ascii_digit()))
    {
        return None;
    }
    Some(DateParts {
        first: first.parse().ok()?,
        second: second.parse().ok()?,
        third: third.parse().ok()?,
        first_wide: first.len() == 4,
        dotted: separator == '.',
    })
}

/// A calendar date from three numbers, with the two-digit year a portal still
/// writes read as this century.
fn build_date(year: u32, month: u32, day: u32) -> std::result::Result<Date, String> {
    let year = match year {
        0..=69 => 2000 + year,
        70..=99 => 1900 + year,
        1900..=2999 => year,
        _ => return Err("names a year that is not one".to_owned()),
    };
    let month = u8::try_from(month)
        .ok()
        .and_then(|month| Month::try_from(month).ok())
        .ok_or_else(|| "names a month that is not one".to_owned())?;
    let day = u8::try_from(day).map_err(|_| "names a day that is not one".to_owned())?;
    let year = i32::try_from(year).map_err(|_| "names a year that is not one".to_owned())?;
    Date::from_calendar_date(year, month, day)
        .map_err(|_| "names a day that is not in that month".to_owned())
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used, clippy::unwrap_used)]

    use super::*;
    use crate::csv_read::parse as parse_csv;

    fn table(text: &str) -> CsvTable {
        parse_csv(text.as_bytes(), 100).expect("a readable table")
    }

    fn request(mapping: BankCsvMapping) -> BankImportRequest {
        BankImportRequest {
            source: Some(BankSource::Csv),
            account_iban: "DE02120300000000202051".to_owned(),
            currency: None,
            dates: BankCsvDates::Auto,
            decimal: BankCsvDecimal::Auto,
            mapping,
        }
    }

    fn read(text: &str, request: &BankImportRequest) -> Result<CsvReading> {
        read_csv_statement(&table(text), request)
    }

    const GERMAN: &str = "Buchungstag;Wertstellung;Verwendungszweck;Empfänger;IBAN;Betrag;Währung\n\
         05.01.2026;05.01.2026;Rechnung INV-2026-00001;Muster GmbH;DE02120300000000202051;1.234,56;EUR\n\
         07.01.2026;08.01.2026;Miete Januar;Vermieter AG;;-800,00;EUR\n";

    #[test]
    fn a_german_export_reads_without_a_mapping_at_all() {
        let reading = read(GERMAN, &request(BankCsvMapping::default())).expect("a reading");
        assert!(reading.errors.is_empty(), "{:?}", reading.errors);
        assert_eq!(
            reading.dates,
            BankCsvDates::Dmy,
            "dots are never month-first"
        );
        let statement = reading.statement.expect("a statement");
        assert_eq!(statement.source, BankSource::Csv);
        assert_eq!(statement.lines.len(), 2);
        assert_eq!(statement.lines[0].amount_cents, 123_456);
        assert_eq!(statement.lines[1].amount_cents, -80_000);
        assert_eq!(statement.lines[0].remittance, "Rechnung INV-2026-00001");
        assert_eq!(statement.lines[1].value_on.to_string(), "2026-01-08");
        assert_eq!(statement.from_date.to_string(), "2026-01-05");
        assert_eq!(statement.to_date.to_string(), "2026-01-07");
        assert_eq!(statement.currency, "EUR");
        assert_eq!(
            reading.at,
            vec![2, 3],
            "the file lines, as a spreadsheet counts"
        );
        // The guess a wizard shows, named in the file's own words.
        assert_eq!(reading.mapping.booked_on.as_deref(), Some("Buchungstag"));
        assert_eq!(reading.mapping.amount.as_deref(), Some("Betrag"));
        assert_eq!(reading.mapping.value_on.as_deref(), Some("Wertstellung"));
    }

    #[test]
    fn a_debit_and_credit_pair_becomes_one_signed_amount() {
        let reading = read(
            "Date,Description,Paid out,Paid in\n\
             2026-02-03,Coffee,3.40,\n\
             2026-02-04,Invoice,,1200.00\n",
            &request(BankCsvMapping::default()),
        )
        .expect("a reading");
        let statement = reading.statement.expect("a statement");
        assert_eq!(statement.lines[0].amount_cents, -340);
        assert_eq!(statement.lines[1].amount_cents, 120_000);
    }

    #[test]
    fn a_sign_column_decides_the_direction_of_an_unsigned_amount() {
        let reading = read(
            "Datum;Betrag;Soll/Haben;Verwendungszweck\n\
             05.01.2026;120,00;S;Strom\n\
             06.01.2026;90,00;H;Erstattung\n",
            &request(BankCsvMapping::default()),
        )
        .expect("a reading");
        let statement = reading.statement.expect("a statement");
        assert_eq!(statement.lines[0].amount_cents, -12_000);
        assert_eq!(statement.lines[1].amount_cents, 9_000);
    }

    #[test]
    fn a_row_that_cannot_be_read_names_its_line_and_stops_the_file() {
        let reading = read(
            "Date,Amount,Description\n\
             2026-02-03,3.40,Coffee\n\
             not-a-date,9.00,Lunch\n\
             2026-02-05,nine euros,Dinner\n",
            &request(BankCsvMapping::default()),
        )
        .expect("a reading");
        assert!(reading.statement.is_none(), "nothing is staged");
        assert_eq!(reading.errors.len(), 2);
        assert_eq!(reading.errors[0].line, 3);
        assert_eq!(reading.errors[1].line, 4);
        for error in &reading.errors {
            assert!(
                !error.rule.contains("Lunch") && !error.rule.contains("nine euros"),
                "a rule never quotes the row: {}",
                error.rule
            );
        }
    }

    #[test]
    fn a_row_blank_in_every_mapped_column_is_skipped_and_counted() {
        let reading = read(
            "Date,Amount,Description,Note\n\
             2026-02-03,3.40,Coffee,\n\
             ,,,Saldo\n",
            &request(BankCsvMapping {
                booked_on: Some("Date".to_owned()),
                amount: Some("Amount".to_owned()),
                remittance: Some("Description".to_owned()),
                ..BankCsvMapping::default()
            }),
        )
        .expect("a reading");
        assert_eq!(reading.skipped, vec![3]);
        assert_eq!(reading.statement.expect("a statement").lines.len(), 1);
    }

    #[test]
    fn an_ambiguous_amount_is_refused_until_the_convention_is_stated() {
        let file = "Date,Amount\n2026-02-03,1.234\n";
        let reading = read(file, &request(BankCsvMapping::default())).expect("a reading");
        assert_eq!(reading.errors.len(), 1, "1.234 is a factor of a thousand");
        assert!(reading.errors[0].rule.contains("comma or a dot"));

        let stated = BankImportRequest {
            decimal: BankCsvDecimal::Comma,
            ..request(BankCsvMapping::default())
        };
        let reading = read(file, &stated).expect("a reading");
        let statement = reading.statement.expect("a statement");
        assert_eq!(
            statement.lines[0].amount_cents, 123_400,
            "1.234 is a thousand"
        );
    }

    #[test]
    fn an_ambiguous_date_column_is_refused_until_the_order_is_stated() {
        let file = "Date,Amount\n03/04/2026,10.00\n05/06/2026,20.00\n";
        let refused = read(file, &request(BankCsvMapping::default())).expect_err("ambiguous");
        assert!(
            matches!(&refused, StoreError::Validation(message) if message.contains("date order")),
            "{refused:?}"
        );
        let stated = BankImportRequest {
            dates: BankCsvDates::Mdy,
            ..request(BankCsvMapping::default())
        };
        let reading = read(file, &stated).expect("a reading");
        let statement = reading.statement.expect("a statement");
        assert_eq!(statement.lines[0].booked_on.to_string(), "2026-03-04");
    }

    #[test]
    fn one_unambiguous_row_settles_the_order_for_the_whole_file() {
        let reading = read(
            "Date,Amount\n03/04/2026,10.00\n21/04/2026,20.00\n",
            &request(BankCsvMapping::default()),
        )
        .expect("a reading");
        assert_eq!(reading.dates, BankCsvDates::Dmy);
        let statement = reading.statement.expect("a statement");
        assert_eq!(statement.lines[0].booked_on.to_string(), "2026-04-03");
    }

    #[test]
    fn a_file_that_disagrees_with_itself_about_dates_is_refused() {
        let refused = read(
            "Date,Amount\n21/04/2026,10.00\n04/21/2026,20.00\n",
            &request(BankCsvMapping::default()),
        )
        .expect_err("two orders");
        assert!(
            matches!(&refused, StoreError::Validation(message)
                if message.contains("not all written the same way round")),
            "{refused:?}"
        );
    }

    #[test]
    fn an_iso_date_is_read_as_iso_even_in_a_day_first_file() {
        let stated = BankImportRequest {
            dates: BankCsvDates::Dmy,
            ..request(BankCsvMapping::default())
        };
        let reading = read("Date,Amount\n2026-01-05,10.00\n", &stated).expect("a reading");
        let statement = reading.statement.expect("a statement");
        assert_eq!(statement.lines[0].booked_on.to_string(), "2026-01-05");
    }

    #[test]
    fn a_mapping_pointing_at_a_column_the_file_lacks_is_refused_before_a_row_is_read() {
        let refused = read(
            "Date,Amount\n2026-01-05,10.00\n",
            &request(BankCsvMapping {
                booked_on: Some("Date".to_owned()),
                amount: Some("Montant".to_owned()),
                ..BankCsvMapping::default()
            }),
        )
        .expect_err("no such column");
        assert!(
            matches!(&refused, StoreError::Validation(message) if message.contains("the amount")),
            "{refused:?}"
        );
    }

    #[test]
    fn a_file_with_no_amount_column_at_all_is_refused_with_the_fix() {
        let refused = read(
            "Date,Note\n2026-01-05,hello\n",
            &request(BankCsvMapping::default()),
        )
        .expect_err("no amount");
        assert!(
            matches!(&refused, StoreError::Validation(message)
                if message.contains("which column holds the amount")),
            "{refused:?}"
        );
    }

    #[test]
    fn a_row_stating_both_a_debit_and_a_credit_is_a_row_error() {
        let reading = read(
            "Date,Paid out,Paid in\n2026-01-05,10.00,20.00\n",
            &request(BankCsvMapping::default()),
        )
        .expect("a reading");
        assert_eq!(reading.errors.len(), 1);
        assert!(reading.errors[0].rule.contains("both a debit and a credit"));
    }

    #[test]
    fn the_trailing_minus_and_the_parenthesis_are_the_same_minus() {
        assert_eq!(
            read_cents("1.234,56-", BankCsvDecimal::Comma).expect("a number"),
            Some(-123_456)
        );
        assert_eq!(
            read_cents("(120,00)", BankCsvDecimal::Comma).expect("a number"),
            Some(-12_000)
        );
        assert_eq!(
            read_cents("+120.00", BankCsvDecimal::Dot).expect("a number"),
            Some(12_000)
        );
        assert_eq!(
            read_cents("   ", BankCsvDecimal::Auto).expect("blank"),
            None
        );
    }

    #[test]
    fn a_stated_convention_reads_the_other_separator_as_grouping() {
        // With the dot as the decimal separator, `1,234` is a thousand — the
        // reading `Auto` refuses to choose between.
        assert_eq!(
            read_cents("1,234", BankCsvDecimal::Dot).expect("a number"),
            Some(123_400)
        );
        let refused = read_cents("1.2345", BankCsvDecimal::Dot).expect_err("three decimals");
        assert!(refused.contains("more than two decimals"), "{refused}");
    }

    #[test]
    fn a_zero_row_is_not_a_transaction() {
        let reading = read(
            "Date,Amount\n2026-01-05,0.00\n",
            &request(BankCsvMapping::default()),
        )
        .expect("a reading");
        assert_eq!(reading.errors.len(), 1);
        assert!(reading.errors[0].rule.contains("moves nothing"));
    }

    #[test]
    fn a_stated_currency_is_the_statements_and_a_column_overrides_it_per_row() {
        let reading = read(
            "Date,Amount,Ccy\n2026-01-05,10.00,USD\n2026-01-06,20.00,\n",
            &BankImportRequest {
                currency: Some("chf".to_owned()),
                mapping: BankCsvMapping {
                    booked_on: Some("Date".to_owned()),
                    amount: Some("Amount".to_owned()),
                    currency: Some("Ccy".to_owned()),
                    ..BankCsvMapping::default()
                },
                ..request(BankCsvMapping::default())
            },
        )
        .expect("a reading");
        let statement = reading.statement.expect("a statement");
        assert_eq!(statement.currency, "CHF");
        assert_eq!(statement.lines[0].currency, "USD");
        assert_eq!(statement.lines[1].currency, "CHF");
    }

    #[test]
    fn a_file_of_nothing_but_blank_rows_is_refused_rather_than_staged_empty() {
        let refused = read(
            "Date,Amount,Note\n,,Saldo\n",
            &request(BankCsvMapping {
                booked_on: Some("Date".to_owned()),
                amount: Some("Amount".to_owned()),
                ..BankCsvMapping::default()
            }),
        )
        .expect_err("nothing to stage");
        assert!(
            matches!(&refused, StoreError::Validation(message)
                if message.contains("no transactions")),
            "{refused:?}"
        );
    }
}
