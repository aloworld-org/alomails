//! Reading SWIFT MT940 — the bank statement that predates ISO 20022 and
//! outlives it (alo Finance, ADR 0035, wave B4.08b; `docs/design/finance.md`,
//! "The bank").
//!
//! One job: walk a tagged SWIFT message into the format-free
//! [`crate::bank_import::ParsedStatement`], which is then validated and staged
//! by exactly the rules a CAMT.053 goes through. It decides nothing about what
//! a bank line *means* — that is [`crate::bank_import`]'s job — and it shares
//! no code with [`crate::bank_camt`] beyond the amount and date readers,
//! because the two formats have nothing in common but their subject.
//!
//! # What MT940 is
//!
//! A flat file of fields, each opening with `:TAG:` at the start of a line and
//! running until the next one. A statement is `:20:` (its reference), `:25:`
//! (the account), `:28C:` (its number), `:60F:` (the opening balance), then a
//! `:61:`/`:86:` pair per transaction, then `:62F:` (the closing balance). The
//! file may be wrapped in SWIFT's own `{1:}{2:}{4: … -}` blocks, and a long
//! statement may be **paged**: `:62M:` says "more to come", and the next page
//! reopens with `:60M:`.
//!
//! # The five things this file decides
//!
//! **The sign.** `:61:` states `C` or `D`, and `RC`/`RD` for the reversal of
//! either. Money in is positive, money out is negative, and after this module
//! nothing in alo re-decides it.
//!
//! **Which date is which.** `:61:` opens with the **value** date and then,
//! optionally, the **entry** date — the opposite order to how a person says it.
//! The entry date is the day the bank posted the transaction and is therefore
//! `booked_on`, the day the books use; the value date is `value_on`. An entry
//! date states no year, so it takes the one that puts it nearest its own value
//! date: `:61:2601011231…` is booked on the last day of the *old* year, eleven
//! months before the January its value date names.
//!
//! **Who the counterparty is.** The standard has no field for one. German
//! banks put it in `:86:` as `?`-coded subfields — `?32`/`?33` the name, `?31`
//! the account, `?20`–`?29` what was written on the payment — and where a bank
//! sends free text instead, the whole of `:86:` is the remittance and the
//! counterparty is blank. A blank field is the honest answer; inventing one
//! would be a false statement on the screen where a human decides what a
//! payment was.
//!
//! **That the `?2n` chunks are one string.** They are 27-character slices of a
//! single remittance, and a bank splits them mid-word without apology. They are
//! therefore joined with **nothing at all**, which reconstructs the string the
//! payer typed — including an invoice number split across two chunks, which is
//! exactly the string B4.09 will search. Joining with a space would break that
//! number in half for ever.
//!
//! **What a paged statement is.** One statement, not two. A file that closes
//! `:62F:` and then opens another `:20:` is two statements and is refused, for
//! the reason a multi-`Stmt` CAMT is: they are usually two accounts, and
//! staging the first silently would put one account's lines on screen and lose
//! the rest.

use time::Date;

use crate::bank_import::{
    BankSource, MAX_BANK_FILE_BYTES, ParsedLine, ParsedStatement, STATEMENT_LINES_MAX,
};
use crate::billing_einvoice_import::amount as decimal_amount;
use crate::error::{Result, StoreError};

/// Reads an uploaded MT940 file.
///
/// The bytes are the file as the bank's portal downloads it — `.sta`, `.940`,
/// `.txt`, with or without SWIFT's envelope blocks. Text is read as UTF-8 and,
/// failing that, as Windows-1252: MT940's own character set has no umlauts and
/// German banks write them anyway, and losing a month's lines over `ä` would be
/// pedantry with a business cost.
///
/// # Errors
/// [`StoreError::Validation`] for every failure: too large, a ZIP, not a
/// tagged SWIFT message, no account, more than one statement in one file, or a
/// `:61:` we cannot read exactly. The message names the transaction and the
/// field and **never quotes the file** — a bank statement is the tenant's money
/// moving, and error text is not a place we put it (Law 1).
pub fn parse_mt940(bytes: &[u8]) -> Result<ParsedStatement> {
    if bytes.len() > MAX_BANK_FILE_BYTES {
        return Err(StoreError::Validation(format!(
            "a bank statement file must be at most {} MB",
            MAX_BANK_FILE_BYTES / (1024 * 1024)
        )));
    }
    if bytes.starts_with(b"PK\x03\x04") {
        return Err(StoreError::Validation(
            "this is a ZIP archive. Banks often bundle a month of statements: unpack it and \
             upload the MT940 files one at a time"
                .to_owned(),
        ));
    }
    let (text, _) = crate::csv_read::decode(bytes);
    let fields = fields(strip_envelope(&text));
    if fields.is_empty() {
        return Err(StoreError::Validation(
            "this file is not an MT940 statement: the form is SWIFT fields, each opening with a \
             tag of its own (:20:, :25:, :28C:, :60F:, :61:, :62F:)"
                .to_owned(),
        ));
    }
    read_statement(&fields)
}

/// One SWIFT field: its tag and everything up to the next one, continuation
/// lines joined by newlines.
#[derive(Debug, PartialEq, Eq)]
struct Field {
    tag: String,
    content: String,
}

/// Drops SWIFT's transport envelope, when the file carries one.
///
/// A file downloaded from a bank portal is usually bare text block 4; one
/// pulled off an interface is wrapped in `{1:…}{2:…}{4: … -}` with a `{5:…}`
/// trailer. Only block 4 is the statement, and the block markers would
/// otherwise be read as the first field's content.
fn strip_envelope(text: &str) -> &str {
    let Some(start) = text.find("{4:") else {
        return text;
    };
    let body = &text[start + "{4:".len()..];
    // Block 4 ends at a line holding only "-}"; a file truncated before it is
    // still read to its end rather than thrown away.
    match body.find("\n-}") {
        Some(end) => &body[..end],
        None => body,
    }
}

/// Splits the text into fields.
///
/// A field opens with `:TAG:` at the start of a line; every line that does not
/// belongs to the field before it. Anything before the first tag — a bank's
/// covering line, an envelope remnant — is not a field and is dropped.
fn fields(text: &str) -> Vec<Field> {
    let mut fields: Vec<Field> = Vec::new();
    for raw in text.lines() {
        let line = raw.strip_suffix('\r').unwrap_or(raw);
        match tag_of(line) {
            Some((tag, rest)) => fields.push(Field {
                tag: tag.to_owned(),
                content: rest.to_owned(),
            }),
            None => {
                if let Some(open) = fields.last_mut() {
                    open.content.push('\n');
                    open.content.push_str(line);
                }
            }
        }
    }
    fields
}

/// The tag a line opens with (`:61:` → `61`) and the rest of the line, or
/// `None` when the line is a continuation.
///
/// A tag is digits and at most one trailing letter — `20`, `28C`, `60F`, `90D`
/// — which is what keeps a free-text `:86:` line beginning with a colon from
/// being read as a new field.
fn tag_of(line: &str) -> Option<(&str, &str)> {
    let rest = line.strip_prefix(':')?;
    let end = rest.find(':')?;
    let tag = &rest[..end];
    let digits = tag.trim_end_matches(|c: char| c.is_ascii_alphabetic());
    let letters = tag.len() - digits.len();
    let shaped = (2..=3).contains(&digits.len())
        && digits.chars().all(|c| c.is_ascii_digit())
        && letters <= 1;
    shaped.then(|| (tag, &rest[end + 1..]))
}

/// Which of the two things a closing balance says about what follows it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Closing {
    /// `:62F:` — this statement is complete.
    Final,
    /// `:62M:` — this is a page of a longer statement, and another follows.
    Intermediate,
}

/// A balance as `:60F:`/`:62F:` state it: the signed figure and its day. Its
/// currency travels separately, because it belongs to the statement rather than
/// to either balance and MT940 states it on both.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Balance {
    cents: i64,
    on: Date,
}

/// Walks the fields of one statement.
fn read_statement(fields: &[Field]) -> Result<ParsedStatement> {
    let mut account: Option<String> = None;
    let mut account_currency = String::new();
    let mut balance_currency = String::new();
    let mut statement_ref = String::new();
    let mut opening: Option<Balance> = None;
    let mut closing: Option<Balance> = None;
    let mut last_closing: Option<Closing> = None;
    let mut lines: Vec<ParsedLine> = Vec::new();
    // Whether the field just read was a `:61:`, so that a `:86:` belongs to it.
    // A `:86:` after a balance is the bank's note about the statement as a
    // whole, and attaching it to the last transaction would put the wrong words
    // beside somebody's money.
    let mut open_line = false;

    for field in fields {
        match field.tag.as_str() {
            "20" => {
                // A second statement reference after a *final* closing balance
                // is a second statement. After an intermediate one it is the
                // next page of this statement, which is ordinary.
                if last_closing == Some(Closing::Final) {
                    return Err(two_statements());
                }
                open_line = false;
            }
            "25" | "25P" => {
                let (iban, currency) = account_of(&field.content)?;
                if let Some(held) = &account {
                    if *held != iban {
                        // A page that names another account is not a page.
                        return Err(two_statements());
                    }
                } else {
                    account = Some(iban);
                }
                if account_currency.is_empty() {
                    account_currency = currency;
                }
                open_line = false;
            }
            "28" | "28C" => {
                if statement_ref.is_empty() {
                    statement_ref = field.content.trim().to_owned();
                }
                open_line = false;
            }
            "60F" | "60M" => {
                // The first opening balance of the file: on a paged statement
                // the later `:60M:` restates a running figure, not the period's
                // start.
                let (stated, currency) =
                    balance("the statement's opening balance", &field.content)?;
                if opening.is_none() {
                    opening = Some(stated);
                    balance_currency = currency;
                }
                open_line = false;
            }
            "62F" | "62M" => {
                // The last closing balance of the file: on a paged statement
                // that is the final page's, which is where the period ends.
                let (stated, currency) =
                    balance("the statement's closing balance", &field.content)?;
                closing = Some(stated);
                if balance_currency.is_empty() {
                    balance_currency = currency;
                }
                last_closing = Some(if field.tag == "62F" {
                    Closing::Final
                } else {
                    Closing::Intermediate
                });
                open_line = false;
            }
            "61" => {
                if lines.len() >= STATEMENT_LINES_MAX {
                    return Err(StoreError::Validation(format!(
                        "this statement states more than {STATEMENT_LINES_MAX} transactions; ask \
                         the bank for it a month at a time"
                    )));
                }
                lines.push(read_line(lines.len() + 1, &field.content)?);
                open_line = true;
            }
            "86" => {
                if let Some(line) = lines.last_mut().filter(|_| open_line) {
                    describe(line, &field.content);
                }
                // Deliberately not `open_line = false`: `:86:` describes the
                // `:61:` above it, and a bank that states two of them is still
                // describing that transaction. The later reading wins — one
                // description of a line, and the fullest one the file offers.
            }
            // `:21:` related reference, `:64:`/`:65:` available balances,
            // `:90D:`/`:90C:` turnover counts, and whatever else a bank adds.
            // None of them says what a transaction was, and refusing a file
            // over one would lose the month.
            _ => open_line = false,
        }
    }

    let account_iban = account.ok_or_else(|| {
        StoreError::Validation(
            "this statement names no account (:25:), so we cannot tell whose money it is"
                .to_owned(),
        )
    })?;
    let currency = currency_of(&balance_currency, &account_currency)?;
    let (from_date, to_date) = period(opening, closing, &lines)?;
    let lines = lines
        .into_iter()
        .map(|line| ParsedLine {
            currency: currency.clone(),
            ..line
        })
        .collect();

    Ok(ParsedStatement {
        source: BankSource::Mt940,
        account_iban,
        currency,
        statement_ref,
        opening_balance_cents: opening.map(|balance| balance.cents),
        closing_balance_cents: closing.map(|balance| balance.cents),
        from_date,
        to_date,
        lines,
        // MT940 has no notion of an unbooked entry: a statement states what the
        // bank booked. Nothing is skipped, so nothing is counted.
        unbooked: 0,
    })
}

/// The refusal for a file holding more than one statement.
fn two_statements() -> StoreError {
    StoreError::Validation(
        "this file carries more than one statement. Import them one at a time, so each is \
         recorded against the account it belongs to"
            .to_owned(),
    )
}

/// The account `:25:` names, and the currency it states beside it.
///
/// Banks write this field four ways: the IBAN alone, the IBAN with its currency
/// appended, a BIC and the IBAN separated by `/`, and — the one we cannot use —
/// a domestic sort code and account number. So every `/`-separated part is
/// offered to [`crate::iban`], which is the crate's one notion of what an IBAN
/// is, with and without a three-letter currency suffix; the first part that is
/// an IBAN is the account.
///
/// # Errors
/// [`StoreError::Validation`] when no part of the field is an IBAN. A staged
/// line is keyed to the account it moved on, so a statement of an account we
/// cannot name is not something to import half of. The message says what is
/// missing and never repeats the number.
fn account_of(content: &str) -> Result<(String, String)> {
    let mut account: Option<String> = None;
    let mut currency = String::new();
    for part in content.trim().split('/') {
        let part = part.trim();
        match account_part(part) {
            Some((iban, appended)) if account.is_none() => {
                account = Some(iban);
                if !appended.is_empty() {
                    currency = appended;
                }
            }
            // `DE02…/EUR`: the currency is a part of its own.
            _ if currency.is_empty() && is_currency(part) => currency = part.to_uppercase(),
            _ => {}
        }
    }
    if let Some(iban) = account {
        return Ok((iban, currency));
    }
    Err(StoreError::Validation(
        "this statement's account (:25:) is not stated as an IBAN. Ask the bank for the SEPA \
         format of the file: a bank line is filed under the account it moved on, and a domestic \
         account number cannot be told from another bank's"
            .to_owned(),
    ))
}

/// The IBAN one `/`-separated part of `:25:` states, and the currency appended
/// to it, or `None` when the part is not an account we can read.
fn account_part(part: &str) -> Option<(String, String)> {
    if let Ok(Some(iban)) = crate::iban::canonicalize(part) {
        return Some((iban, String::new()));
    }
    // `NL91ABNA0417164300EUR`: the currency is appended to the account, and
    // only stripping it reveals an IBAN.
    let (head, tail) = part.split_at_checked(part.len().saturating_sub(3))?;
    match (is_currency(tail), crate::iban::canonicalize(head)) {
        (true, Ok(Some(iban))) => Some((iban, tail.to_uppercase())),
        _ => None,
    }
}

/// Whether a part of a field is a three-letter currency code.
fn is_currency(part: &str) -> bool {
    part.len() == 3 && part.chars().all(|c| c.is_ascii_alphabetic())
}

/// Reads a `:60F:`/`:62F:` balance: `C260131EUR7910,10` — the mark, the day,
/// the currency, the figure. Returns the balance and the currency it names.
///
/// # Errors
/// [`StoreError::Validation`] naming the balance, never its value.
fn balance(term: &str, content: &str) -> Result<(Balance, String)> {
    let text = content.trim();
    let unreadable = || StoreError::Validation(format!("{term} is not a balance we can read"));
    let (mark, rest) = text.split_at_checked(1).ok_or_else(unreadable)?;
    let negative = match mark {
        "C" => false,
        "D" => true,
        _ => return Err(unreadable()),
    };
    let (day, rest) = rest.split_at_checked(6).ok_or_else(unreadable)?;
    let (currency, figure) = rest.split_at_checked(3).ok_or_else(unreadable)?;
    if !is_currency(currency) {
        return Err(unreadable());
    }
    let cents = decimal_amount(term, &figure.replace(',', "."))?.abs();
    Ok((
        Balance {
            cents: if negative { -cents } else { cents },
            on: six_digit_day(term, day)?,
        },
        currency.to_uppercase(),
    ))
}

/// The statement's currency: what its balances state, or — for a page that
/// states no balance at all — what `:25:` wrote beside the account.
///
/// # Errors
/// [`StoreError::Validation`] when the file names none. MT940 states no
/// currency on a transaction, so there is nowhere else to look, and a statement
/// whose currency we guessed would be money in the wrong unit.
fn currency_of(balance_currency: &str, account_currency: &str) -> Result<String> {
    for stated in [balance_currency, account_currency] {
        if !stated.is_empty() {
            return Ok(stated.to_owned());
        }
    }
    Err(StoreError::Validation(
        "this statement names no currency, on either balance (:60F:, :62F:) or on the account \
         (:25:)"
            .to_owned(),
    ))
}

/// The period the statement covers.
///
/// The balance days are the bank's own account of it — but `:60F:` often
/// carries the day the *previous* statement closed, and a paged file can state
/// a closing balance dated before its last transaction. So the period is
/// widened to hold every line it stages: a statement whose own transactions
/// fall outside its period would be a statement that lies about itself.
///
/// # Errors
/// [`StoreError::Validation`] when the file states neither a balance nor a
/// transaction — nothing to take a period from, and stamping it with today
/// would file an empty January under August.
fn period(
    opening: Option<Balance>,
    closing: Option<Balance>,
    lines: &[ParsedLine],
) -> Result<(Date, Date)> {
    let mut days: Vec<Date> = lines.iter().map(|line| line.booked_on).collect();
    let stated = (
        opening.map(|balance| balance.on),
        closing.map(|balance| balance.on),
    );
    days.extend([stated.0, stated.1].into_iter().flatten());
    days.sort_unstable();
    match (days.first(), days.last()) {
        (Some(from), Some(to)) => Ok((*from, *to)),
        _ => Err(StoreError::Validation(
            "this statement states neither a balance nor a single transaction, so there is \
             nothing in it to import"
                .to_owned(),
        )),
    }
}

/// Reads one `:61:` into a line. `at` is its 1-based position, and it is what
/// every refusal names.
///
/// The field is positional, in this order: the value date, an optional entry
/// date, the credit/debit mark, an optional funds code, the amount, a
/// transaction type, the account owner's reference, and — after `//` — the
/// bank's own. A second line, when the bank writes one, is supplementary
/// detail.
fn read_line(at: usize, content: &str) -> Result<ParsedLine> {
    let term = format!("transaction {at} of this statement");
    let mut rest = content.trim_start();
    let supplementary = match rest.split_once('\n') {
        Some((head, tail)) => {
            rest = head;
            tail.replace('\n', " ")
        }
        None => String::new(),
    };
    let rest = rest.trim();

    let (value_raw, rest) = rest
        .split_at_checked(6)
        .ok_or_else(|| unreadable_line(&term))?;
    let value_on = six_digit_day(&format!("{term}: the value date"), value_raw)?;

    // An entry date is four digits and no year; the mark that would otherwise
    // stand here is always a letter, so digits can only be the date.
    let (entry_raw, rest) = match rest.split_at_checked(4) {
        Some((head, tail)) if head.chars().all(|c| c.is_ascii_digit()) => (Some(head), tail),
        _ => (None, rest),
    };
    let booked_on = match entry_raw {
        Some(raw) => entry_day(&format!("{term}: the entry date"), raw, value_on)?,
        None => value_on,
    };

    let (inbound, rest) = if let Some(rest) = rest.strip_prefix("RC") {
        // A reversed credit is money leaving, whatever the letter says.
        (false, rest)
    } else if let Some(rest) = rest.strip_prefix("RD") {
        (true, rest)
    } else if let Some(rest) = rest.strip_prefix('C') {
        (true, rest)
    } else if let Some(rest) = rest.strip_prefix('D') {
        (false, rest)
    } else {
        return Err(StoreError::Validation(format!(
            "{term} does not say whether the money came in or went out (the C/D mark of :61:), and \
             that is not something to guess at"
        )));
    };
    // The funds code, when a bank states one, is the currency's third letter
    // and says nothing we do not already know.
    let rest = match rest.strip_prefix(|c: char| c.is_ascii_alphabetic()) {
        Some(shorter) => shorter,
        None => rest,
    };

    let figure: String = rest
        .chars()
        .take_while(|c| c.is_ascii_digit() || *c == ',')
        .collect();
    let rest = &rest[figure.len()..];
    let magnitude = decimal_amount(&term, &figure.replace(',', "."))?.abs();

    // The transaction type — `NTRF`, `NDDT`, `NMSC` — classifies the movement
    // for the bank's own purposes. It is skipped rather than read: what a
    // payment *was* is decided by a human at B4.09, not by a four-letter code
    // whose meaning differs by country.
    let rest = strip_type_code(rest);
    let (owner_ref, bank_ref) = match rest.split_once("//") {
        Some((owner, bank)) => (owner, bank),
        None => (rest, ""),
    };

    Ok(ParsedLine {
        booked_on,
        value_on,
        amount_cents: if inbound { magnitude } else { -magnitude },
        // MT940 states no currency on a transaction; the statement's is filled
        // in once, by the caller, so that one file cannot hold two answers.
        currency: String::new(),
        counterparty_name: String::new(),
        counterparty_iban: String::new(),
        remittance: squash(&supplementary),
        bank_ref: reference(&[bank_ref, owner_ref]),
    })
}

/// The refusal for a `:61:` too short to be one.
fn unreadable_line(term: &str) -> StoreError {
    StoreError::Validation(format!(
        "{term} is not a transaction we can read: a :61: field states a value date, a C/D mark and \
         an amount"
    ))
}

/// Drops the transaction type code, which SWIFT states as `N`, `S` or `F` and
/// three more characters (`NTRF`, `NDDT`, `NMSC`).
///
/// The subfield is mandatory, so a reference that happens to begin the same way
/// — `NONREF` — never stands where it does.
fn strip_type_code(rest: &str) -> &str {
    let bytes = rest.as_bytes();
    let shaped = bytes.len() >= 4
        && matches!(bytes[0], b'N' | b'S' | b'F')
        && bytes[1..4].iter().all(u8::is_ascii_alphanumeric);
    if shaped { &rest[4..] } else { rest }
}

/// The first reference that says something.
///
/// The bank's own comes first: `NONREF` is MT940's way of stating that the
/// payer supplied none, and a file full of `NONREF` would otherwise make every
/// transaction of a busy day look like the same one.
fn reference(candidates: &[&str]) -> String {
    for candidate in candidates {
        let candidate = candidate.trim();
        if !candidate.is_empty()
            && !candidate.eq_ignore_ascii_case("NONREF")
            && !candidate.eq_ignore_ascii_case("NOTPROVIDED")
        {
            return candidate.to_owned();
        }
    }
    String::new()
}

/// Reads `YYMMDD`. MT940 states no century, and has been the SEPA-era file
/// format throughout this one: `26` is 2026.
///
/// # Errors
/// [`StoreError::Validation`] naming the term whose date is unreadable.
fn six_digit_day(term: &str, raw: &str) -> Result<Date> {
    let text = raw.trim();
    if text.len() != 6 || !text.chars().all(|c| c.is_ascii_digit()) {
        return Err(StoreError::Validation(format!(
            "{term} is not a date of the form YYMMDD"
        )));
    }
    crate::billing_einvoice_import::date(term, &format!("20{text}"))
}

/// Reads an entry date's `MMDD` into the year that puts it nearest its own
/// value date.
///
/// A statement written on 1 January states last year's bookings with no year at
/// all, and reading `1231` as this December would file a payment eleven months
/// late. The nearest of the three candidate years is the only reading that is
/// right on both sides of a year boundary.
///
/// # Errors
/// [`StoreError::Validation`] when the four digits are not a month and a day.
fn entry_day(term: &str, raw: &str, value_on: Date) -> Result<Date> {
    let text = raw.trim();
    if text.len() != 4 || !text.chars().all(|c| c.is_ascii_digit()) {
        return Err(StoreError::Validation(format!(
            "{term} is not a date of the form MMDD"
        )));
    }
    let year = value_on.year();
    let candidates: Vec<Date> = [year - 1, year, year + 1]
        .into_iter()
        .filter_map(|candidate| {
            crate::billing_einvoice_import::date(term, &format!("{candidate:04}{text}")).ok()
        })
        .collect();
    candidates
        .into_iter()
        .min_by_key(|day| (*day - value_on).whole_days().abs())
        .ok_or_else(|| StoreError::Validation(format!("{term} is not a day of any month (MMDD)")))
}

/// Reads `:86:` onto the line it belongs to.
///
/// Two shapes, and the file says which: `?nn`-coded subfields, which German
/// banks fill with everything a reconciliation screen wants, or free text,
/// which is the whole remittance and no counterparty.
fn describe(line: &mut ParsedLine, content: &str) {
    // A bank breaks a line wherever its own width runs out, including in the
    // middle of a subfield's value, so a newline inside a structured field is
    // layout and nothing else.
    let flat = content.replace('\n', "");
    if let Some(coded) = subfields(&flat) {
        let mut remittance = String::new();
        let mut fallback = String::new();
        let mut name = String::new();
        for (code, value) in coded {
            match code {
                // The 27-character slices of one string, joined with nothing:
                // reconstructing what the payer typed is what keeps an invoice
                // number split across two of them findable.
                20..=29 | 60..=63 => remittance.push_str(value),
                32 | 33 => name.push_str(value),
                31 => line.counterparty_iban = value.trim().to_owned(),
                // The posting text — "SEPA-GUTSCHRIFT", "LASTSCHRIFT" — which
                // is all a bank charge or a batch ever says about itself.
                0 => fallback.push_str(value),
                _ => {}
            }
        }
        if remittance.trim().is_empty() {
            remittance = fallback;
        }
        line.remittance = squash(&remittance);
        line.counterparty_name = squash(&name);
        return;
    }
    // Free text. Its line breaks are the bank's line width, not the payer's
    // meaning, so they read as spaces.
    line.remittance = squash(content);
}

/// The `?nn` subfields of a structured `:86:`, in the order the bank stated
/// them, or `None` when the field is free text — which is anything the coded
/// form does not fit exactly, down to a question mark a payer typed.
fn subfields(flat: &str) -> Option<Vec<(u8, &str)>> {
    let mut coded: Vec<(u8, &str)> = Vec::new();
    let mut parts = flat.split('?');
    // Whatever stands before the first `?` is the bank's transaction code, not
    // a subfield.
    parts.next()?;
    for part in parts {
        let (code, value) = part.split_at_checked(2)?;
        let code: u8 = code.parse().ok()?;
        coded.push((code, value));
    }
    (!coded.is_empty()).then_some(coded)
}

/// Collapses the whitespace a bank's line width introduced, so that one string
/// broken over three lines reads as the one string it is.
fn squash(raw: &str) -> String {
    raw.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use time::Month;

    /// A minimal but complete MT940, with `body` between the opening and
    /// closing balances — the frame every case below varies.
    fn message(body: &str) -> String {
        format!(
            ":20:STARTUMSATZ\r\n\
             :25:DE02120300000000202051\r\n\
             :28C:00001/001\r\n\
             :60F:C260101EUR12500,00\r\n\
             {body}\r\n\
             :62F:C260131EUR12500,00\r\n"
        )
    }

    fn day_of(year: i32, month: Month, day: u8) -> Date {
        Date::from_calendar_date(year, month, day).unwrap_or(Date::MIN)
    }

    fn parsed(text: &str) -> ParsedStatement {
        match parse_mt940(text.as_bytes()) {
            Ok(statement) => statement,
            other => panic!("expected a statement, got {other:?}"),
        }
    }

    fn refused(text: &str) -> String {
        match parse_mt940(text.as_bytes()) {
            Err(StoreError::Validation(message)) => message,
            other => panic!("expected a Validation refusal, got {other:?}"),
        }
    }

    const CREDIT: &str = ":61:2601040105C1250,00NTRFE2E-1//DE-2026-0105-0001\r\n\
         :86:166?00SEPA-GUTSCHRIFT?20Rechnung INV-2026-00007 ?21vielen Dank\r\n\
         ?30ABNANL2A?31NL91ABNA0417164300?32Kaffeehaus Berlin ?33GmbH";

    #[test]
    fn a_credit_is_money_in_and_its_fields_come_off_the_two_lines() {
        let statement = parsed(&message(CREDIT));
        assert_eq!(statement.source, BankSource::Mt940);
        assert_eq!(statement.account_iban, "DE02120300000000202051");
        assert_eq!(statement.currency, "EUR");
        assert_eq!(statement.statement_ref, "00001/001");
        assert_eq!(statement.unbooked, 0);

        let line = &statement.lines[0];
        assert_eq!(line.amount_cents, 125_000, "a credit is money in");
        assert_eq!(
            line.value_on,
            day_of(2026, Month::January, 4),
            ":61: opens with the value date"
        );
        assert_eq!(
            line.booked_on,
            day_of(2026, Month::January, 5),
            "the entry date is the day the books use"
        );
        assert_eq!(
            line.currency, "EUR",
            "a line takes the statement's currency"
        );
        assert_eq!(line.counterparty_name, "Kaffeehaus Berlin GmbH");
        assert_eq!(line.counterparty_iban, "NL91ABNA0417164300");
        assert_eq!(line.remittance, "Rechnung INV-2026-00007 vielen Dank");
        assert_eq!(
            line.bank_ref, "DE-2026-0105-0001",
            "the bank's own reference is preferred to the payer's"
        );
    }

    #[test]
    fn a_debit_is_money_out_and_a_reversal_turns_either_around() {
        let cases = [
            (":61:2601070107D89,90NDDTNONREF", -8_990),
            (":61:2601070107C89,90NDDTNONREF", 8_990),
            (":61:2601070107RC89,90NDDTNONREF", -8_990),
            (":61:2601070107RD89,90NDDTNONREF", 8_990),
            // A funds code — the currency's third letter — stands between the
            // mark and the figure, and says nothing.
            (":61:2601070107DR89,90NDDTNONREF", -8_990),
        ];
        for (field, expected) in cases {
            let line = &parsed(&message(field)).lines[0];
            assert_eq!(line.amount_cents, expected, "for {field}");
            assert_eq!(line.bank_ref, "", "NONREF is not a reference");
        }
    }

    #[test]
    fn a_transaction_with_no_entry_date_is_booked_on_its_value_date() {
        let line = &parsed(&message(":61:260107C89,90NTRFREF-9")).lines[0];
        assert_eq!(line.booked_on, day_of(2026, Month::January, 7));
        assert_eq!(line.value_on, line.booked_on);
        assert_eq!(
            line.bank_ref, "REF-9",
            "the payer's reference is a fallback"
        );
    }

    #[test]
    fn an_entry_date_takes_the_year_that_puts_it_nearest_its_value_date() {
        let statement = parsed(&message(":61:2601011231C10,00NTRFREF-1"));
        let line = &statement.lines[0];
        assert_eq!(
            line.booked_on,
            day_of(2025, Month::December, 31),
            "a December booking valued on 1 January is last year's"
        );
        assert_eq!(line.value_on, day_of(2026, Month::January, 1));
        assert_eq!(
            statement.from_date,
            day_of(2025, Month::December, 31),
            "the period widens to hold every line it stages"
        );

        // And the ordinary case is not disturbed by the same rule.
        let ordinary = &parsed(&message(":61:2601310131C10,00NTRFREF-1")).lines[0];
        assert_eq!(ordinary.booked_on, day_of(2026, Month::January, 31));

        // The other side of a boundary: a January booking valued in December.
        let ahead = &parsed(&message(":61:2512310101C10,00NTRFREF-1")).lines[0];
        assert_eq!(ahead.booked_on, day_of(2026, Month::January, 1));
    }

    #[test]
    fn the_chunks_of_a_structured_remittance_are_one_string() {
        // The split is at the bank's own 27 characters and lands mid-word;
        // joining with nothing is what puts the invoice number back together.
        let split = ":61:2601280128D45,00NTRFNONREF\r\n\
             :86:191?00SAMMELUEBERWEISUNG?20Rueckbuchung Rechnung INV-?212026-00007";
        let line = &parsed(&message(split)).lines[0];
        assert_eq!(line.remittance, "Rueckbuchung Rechnung INV-2026-00007");
        assert_eq!(
            line.counterparty_name, "",
            "a field that names no party names no party"
        );
    }

    #[test]
    fn a_structured_field_that_states_only_a_posting_text_still_says_something() {
        let charges = ":61:2601310131D30,00NTRFNONREF\r\n:86:808?00ENTGELTABSCHLUSS";
        assert_eq!(
            parsed(&message(charges)).lines[0].remittance,
            "ENTGELTABSCHLUSS"
        );
    }

    #[test]
    fn a_free_text_field_is_the_whole_remittance_and_no_counterparty() {
        let free = ":61:2602030203C500,00NTRFREF-3\r\n\
             :86:Factuur INV-2026-00011\r\n   betaling februari";
        let line = &parsed(&message(free)).lines[0];
        assert_eq!(
            line.remittance, "Factuur INV-2026-00011 betaling februari",
            "a line the bank wrapped is one string"
        );
        assert_eq!(line.counterparty_name, "");
        assert_eq!(line.counterparty_iban, "");
    }

    #[test]
    fn a_transaction_with_no_information_field_falls_back_to_its_own_detail() {
        let supplementary = ":61:2601090109D42,00NTRFNONREF//BANK-77\r\nKartenzahlung Tankstelle";
        let line = &parsed(&message(supplementary)).lines[0];
        assert_eq!(line.remittance, "Kartenzahlung Tankstelle");
        assert_eq!(line.bank_ref, "BANK-77");
    }

    #[test]
    fn a_note_after_the_closing_balance_belongs_to_no_transaction() {
        let text = format!(
            ":20:STARTUMSATZ\r\n:25:DE02120300000000202051\r\n:60F:C260101EUR12500,00\r\n\
             {CREDIT}\r\n:62F:C260131EUR13750,00\r\n:86:Bitte beachten Sie unsere neuen Preise\r\n"
        );
        let statement = parsed(&text);
        assert_eq!(statement.lines.len(), 1);
        assert_eq!(
            statement.lines[0].remittance, "Rechnung INV-2026-00007 vielen Dank",
            "the bank's note about the statement is not what the payer wrote"
        );
    }

    #[test]
    fn the_balances_are_signed_and_the_period_is_theirs() {
        let statement = parsed(&message(CREDIT));
        assert_eq!(statement.opening_balance_cents, Some(1_250_000));
        assert_eq!(statement.closing_balance_cents, Some(1_250_000));
        assert_eq!(statement.from_date, day_of(2026, Month::January, 1));
        assert_eq!(statement.to_date, day_of(2026, Month::January, 31));

        let overdrawn = message(CREDIT).replace(":62F:C260131EUR12500,00", ":62F:D260131EUR480,00");
        assert_eq!(
            parsed(&overdrawn).closing_balance_cents,
            Some(-48_000),
            "an overdrawn account closes on a debit balance"
        );

        let unreadable = message(CREDIT).replace(":62F:C260131", ":62F:X260131");
        assert!(refused(&unreadable).contains("closing balance"));
    }

    #[test]
    fn a_statement_wrapped_in_swift_blocks_reads_the_same_as_a_bare_one() {
        let bare = message(CREDIT);
        let wrapped = format!(
            "{{1:F01BYLADEM1AXXX0000000000}}{{2:O9401200260201BYLADEM1AXXX00000000002602010200N}}\
             {{4:\r\n{bare}-}}\r\n{{5:{{CHK:0123456789AB}}}}"
        );
        assert_eq!(parsed(&wrapped).lines, parsed(&bare).lines);
        assert_eq!(parsed(&wrapped).statement_ref, "00001/001");
    }

    #[test]
    fn a_paged_statement_is_one_statement_and_a_second_one_is_refused() {
        let paged = ":20:PAGE1\r\n:25:DE02120300000000202051\r\n:28C:00002/001\r\n\
             :60F:C260201EUR1000,00\r\n:61:2602030203C500,00NTRFREF-1\r\n:86:Erste Seite\r\n\
             :62M:C260203EUR1500,00\r\n\
             :20:PAGE2\r\n:25:DE02120300000000202051\r\n:28C:00002/002\r\n\
             :60M:C260203EUR1500,00\r\n:61:2602100210D200,00NTRFREF-2\r\n:86:Zweite Seite\r\n\
             :62F:C260228EUR1300,00\r\n";
        let statement = parsed(paged);
        assert_eq!(statement.lines.len(), 2, "two pages are one statement");
        assert_eq!(statement.opening_balance_cents, Some(100_000));
        assert_eq!(statement.closing_balance_cents, Some(130_000));
        assert_eq!(statement.statement_ref, "00002/001");
        assert_eq!(statement.from_date, day_of(2026, Month::February, 1));
        assert_eq!(statement.to_date, day_of(2026, Month::February, 28));

        // A file that closes and then starts again is two statements.
        let two = format!("{}{}", message(CREDIT), message(CREDIT));
        assert!(refused(&two).contains("one at a time"));

        // And a page that names another account is not a page.
        let elsewhere = paged.replace(
            ":20:PAGE2\r\n:25:DE02120300000000202051",
            ":20:PAGE2\r\n:25:NL91ABNA0417164300",
        );
        assert!(refused(&elsewhere).contains("one at a time"));
    }

    #[test]
    fn the_account_is_read_however_the_bank_wrote_the_field() {
        for stated in [
            ":25:DE02120300000000202051",
            ":25:DE02 1203 0000 0000 2020 51",
            ":25:DE02120300000000202051EUR",
            ":25:DE02120300000000202051/EUR",
            ":25:BYLADEM1001/DE02120300000000202051",
            ":25P:DE02120300000000202051",
        ] {
            let text = message(CREDIT).replace(":25:DE02120300000000202051", stated);
            assert_eq!(
                parsed(&text).account_iban,
                "DE02120300000000202051",
                "for {stated}"
            );
        }

        // A domestic sort code and account number is not something we can file
        // a bank line under, and the refusal says what to ask the bank for.
        let domestic =
            message(CREDIT).replace(":25:DE02120300000000202051", ":25:12030000/0202051");
        let message_text = refused(&domestic);
        assert!(message_text.contains(":25:") && message_text.contains("IBAN"));

        let nameless = message(CREDIT).replace(":25:DE02120300000000202051\r\n", "");
        assert!(refused(&nameless).contains("names no account"));
    }

    #[test]
    fn a_transaction_we_cannot_read_is_refused_by_its_number() {
        let no_mark = format!("{CREDIT}\r\n:61:2601070107 89,90NTRFNONREF");
        let message_text = refused(&message(&no_mark));
        assert!(
            message_text.contains("transaction 2"),
            "names the transaction: {message_text}"
        );
        assert!(message_text.contains("C/D mark"));

        let bad_amount = refused(&message(":61:2601070107C89,901NTRFNONREF"));
        assert!(bad_amount.contains("transaction 1"), "{bad_amount}");

        let bad_date = refused(&message(":61:26010xC89,90NTRFNONREF"));
        assert!(bad_date.contains("transaction 1") && bad_date.contains("value date"));

        let short = refused(&message(":61:2601"));
        assert!(short.contains("transaction 1"), "{short}");
    }

    #[test]
    fn a_file_that_is_not_a_statement_says_so() {
        assert!(refused("a covering letter, and nothing else").contains("not an MT940"));
        assert!(refused("<Document><BkToCstmrStmt/></Document>").contains("not an MT940"));

        match parse_mt940(b"PK\x03\x04 and then a zip") {
            Err(StoreError::Validation(text)) => assert!(text.contains("ZIP")),
            other => panic!("expected the ZIP answer, got {other:?}"),
        }
        match parse_mt940(&vec![b'x'; MAX_BANK_FILE_BYTES + 1]) {
            Err(StoreError::Validation(text)) => assert!(text.contains("at most 8 MB")),
            other => panic!("expected the size answer, got {other:?}"),
        }
    }

    #[test]
    fn a_file_written_in_the_encoding_german_banks_use_still_reads() {
        // MT940's own character set has no umlauts; banks write them anyway,
        // and a month of lines is not lost over one byte that is not UTF-8.
        let text = message(":61:2601070107D89,90NTRFNONREF\r\n:86:Stadtwerke M\u{fc}nchen");
        let mut latin1: Vec<u8> = Vec::new();
        for character in text.chars() {
            match u8::try_from(character as u32) {
                Ok(byte) => latin1.push(byte),
                Err(_) => panic!("the fixture is Latin-1 by construction"),
            }
        }
        match parse_mt940(&latin1) {
            Ok(statement) => assert_eq!(statement.lines[0].remittance, "Stadtwerke M\u{fc}nchen"),
            other => panic!("expected a statement, got {other:?}"),
        }
    }

    #[test]
    fn a_statement_with_neither_balance_nor_transaction_is_refused() {
        let empty = ":20:STARTUMSATZ\r\n:25:DE02120300000000202051\r\n:28C:00001/001\r\n";
        assert!(refused(empty).contains("names no currency"));
    }

    #[test]
    fn a_tag_is_read_only_where_one_can_stand() {
        assert_eq!(tag_of(":61:2601"), Some(("61", "2601")));
        assert_eq!(tag_of(":28C:00001/001"), Some(("28C", "00001/001")));
        assert_eq!(
            tag_of(":61X:x"),
            Some(("61X", "x")),
            "a tag we do not handle is still a tag, and ends the field before it"
        );
        assert_eq!(tag_of("Zahlung 12:30:00 Uhr"), None);
        assert_eq!(tag_of(":TRF:something"), None);
        assert_eq!(tag_of(":2:x"), None);
        assert_eq!(tag_of(":61XY:x"), None);
        assert_eq!(tag_of(":Ref 4711: bitte beachten"), None);
    }
}
