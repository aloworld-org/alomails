//! Reading ISO 20022 CAMT.053 — the bank's end-of-day statement (alo Finance,
//! ADR 0035, wave B4.08a; `docs/design/finance.md`, "The bank").
//!
//! One job: walk a `BankToCustomerStatement` document
//! ([`crate::billing_xml_tree`]) into the format-free
//! [`crate::bank_import::ParsedStatement`], which is then validated and staged.
//! It decides nothing about what a bank line *means* — that is
//! [`crate::bank_import`]'s job, and keeping the two apart is what will let
//! MT940 (B4.08b) and mapped CSV (B4.08c) be added without re-deciding
//! anything.
//!
//! The tree it walks is the reader B1.24 hardened for inbound e-invoices: no
//! DTD, no entity expansion, no external fetch, bounded depth and element
//! count, prefixes stripped. That is exactly the tool for a file a bank
//! generated with software we have never seen — and the reason `xmlns` versions
//! are ignored here: `camt.053.001.02`, `.04` and `.08` differ in ways this
//! reader does not look at, and refusing a version we can read perfectly well
//! would be pedantry with a business cost.
//!
//! # The four things this file decides
//!
//! **The sign.** CAMT never signs an amount: it states a positive figure beside
//! a `CdtDbtInd` of `CRDT` or `DBIT`, and a `RvslInd` that can turn either one
//! around. Money in is positive, money out is negative, and after this module
//! nothing in alo re-decides it.
//!
//! **Who the counterparty is.** On a credit it is the *debtor* — the party who
//! paid us; on a debit it is the *creditor*. A file states both roles on every
//! transaction and one of them is always the account holder, so reading the
//! wrong one would fill the reconciliation screen with the tenant's own name.
//!
//! **What a batch is.** One `Ntry` may carry several `TxDtls` — a payroll run,
//! a direct-debit collection — and the bank booked it as **one** movement of
//! money. So it stays one line, at the entry's own total, with no counterparty:
//! inventing one of the several would be a false statement on a screen whose
//! whole purpose is deciding what a payment was.
//!
//! **What is not booked is not staged.** A CAMT.053 is an end-of-day statement
//! and its entries should all be `BOOK`; some banks put pending items in one
//! anyway. A pending item is not something to reconcile against — it may still
//! change or vanish — so it is counted and skipped, never staged, and the
//! import report says how many.

use time::Date;

use crate::bank_import::{
    BankSource, MAX_BANK_FILE_BYTES, ParsedLine, ParsedStatement, STATEMENT_LINES_MAX,
};
use crate::billing_einvoice_import::{amount as decimal_amount, date};
use crate::billing_xml_tree::{self, Element};
use crate::error::{Result, StoreError};

/// Reads an uploaded CAMT.053 file.
///
/// The bytes must be the XML document itself, as the bank's portal downloads
/// it. A ZIP of several statements is refused with an answer that says so
/// rather than a generic "not XML": downloading the archive is the obvious
/// thing to try.
///
/// # Errors
/// [`StoreError::Validation`] for every failure: too large, not UTF-8, not XML,
/// not a CAMT.053, more than one statement in one file, or an entry we cannot
/// read exactly. The message names the entry and the element and **never
/// quotes the file** — a bank statement is the tenant's money moving, and error
/// text is not a place we put it (Law 1).
pub fn parse_camt053(bytes: &[u8]) -> Result<ParsedStatement> {
    if bytes.len() > MAX_BANK_FILE_BYTES {
        return Err(StoreError::Validation(format!(
            "a bank statement file must be at most {} MB",
            MAX_BANK_FILE_BYTES / (1024 * 1024)
        )));
    }
    if bytes.starts_with(b"PK\x03\x04") {
        return Err(StoreError::Validation(
            "this is a ZIP archive. Banks often bundle a month of statements: unpack it and \
             upload the CAMT.053 XML files one at a time"
                .to_owned(),
        ));
    }
    // A UTF-8 BOM is legal in front of an XML document and is not part of it.
    let bytes = bytes.strip_prefix(&[0xEF, 0xBB, 0xBF]).unwrap_or(bytes);
    let text = std::str::from_utf8(bytes).map_err(|_| {
        StoreError::Validation(
            "a CAMT.053 file must be UTF-8 text; this one is not readable as text".to_owned(),
        )
    })?;

    let root = billing_xml_tree::parse(text)?;
    if root.name != "Document" {
        return Err(not_camt());
    }
    let report = root.child("BkToCstmrStmt").ok_or_else(not_camt)?;

    let mut statements = report.children_named("Stmt");
    let statement = statements.next().ok_or_else(|| {
        StoreError::Validation(
            "this CAMT document carries no statement, so there is nothing in it to import"
                .to_owned(),
        )
    })?;
    if statements.next().is_some() {
        // Refused rather than half-imported: a multi-statement file usually
        // means several accounts, and staging the first one silently would put
        // one account's lines on screen and lose the rest.
        return Err(StoreError::Validation(
            "this file carries more than one statement. Import them one at a time, so each is \
             recorded against the account it belongs to"
                .to_owned(),
        ));
    }

    read_statement(statement)
}

/// The refusal for a document that is not a CAMT.053.
fn not_camt() -> StoreError {
    StoreError::Validation(
        "this XML document is not a CAMT.053 bank statement: the form is an ISO 20022 Document \
         carrying a BkToCstmrStmt"
            .to_owned(),
    )
}

/// Reads one `<Stmt>`.
fn read_statement(statement: &Element) -> Result<ParsedStatement> {
    let account = statement.at(&["Acct"]).ok_or_else(|| {
        StoreError::Validation(
            "this statement names no account (Acct), so we cannot tell whose money it is"
                .to_owned(),
        )
    })?;

    let mut lines = Vec::new();
    let mut unbooked = 0usize;
    for (index, entry) in statement.children_named("Ntry").enumerate() {
        let at = index + 1;
        if lines.len() >= STATEMENT_LINES_MAX {
            return Err(StoreError::Validation(format!(
                "this statement states more than {STATEMENT_LINES_MAX} entries; ask the bank for \
                 it a month at a time"
            )));
        }
        // `Sts` is a code in older versions and a `Cd` child in newer ones;
        // both spellings say the same word.
        let status = entry.text_at(&["Sts"]);
        let status = if status.is_empty() {
            entry.text_at(&["Sts", "Cd"])
        } else {
            status
        };
        if !status.is_empty() && !status.eq_ignore_ascii_case("BOOK") {
            unbooked += 1;
            continue;
        }
        lines.push(read_entry(at, entry, account.text_at(&["Ownr", "Nm"]))?);
    }

    let currency = account_currency(account, &lines)?;
    let (from_date, to_date) = period(statement, &lines)?;

    Ok(ParsedStatement {
        source: BankSource::Camt,
        account_iban: account.text_at(&["Id", "IBAN"]).to_owned(),
        currency,
        statement_ref: statement.text_at(&["Id"]).to_owned(),
        // OPBD is the opening booked balance; PRCD ("previously closed
        // booked") is the same figure under the name some banks prefer, and
        // taking it as a fallback is the difference between a statement that
        // reconciles and one that says nothing.
        opening_balance_cents: balance(statement, &["OPBD", "PRCD"])?,
        closing_balance_cents: balance(statement, &["CLBD"])?,
        from_date,
        to_date,
        lines,
        unbooked,
    })
}

/// The statement's currency: what the account states, or — for a file that
/// states none on the account, which some banks send — what its entries agree
/// on.
fn account_currency(account: &Element, lines: &[ParsedLine]) -> Result<String> {
    let stated = account.text_at(&["Ccy"]).trim().to_uppercase();
    if !stated.is_empty() {
        return Ok(stated);
    }
    lines
        .iter()
        .map(|line| line.currency.clone())
        .find(|currency| !currency.is_empty())
        .ok_or_else(|| {
            StoreError::Validation(
                "this statement names no currency, on the account or on any entry".to_owned(),
            )
        })
}

/// The period the statement covers: what `FrToDt` states, or the span of the
/// entries when it states nothing.
///
/// An empty statement with no `FrToDt` is the one case with no answer at all —
/// no dates and no entries to take them from — and it is refused rather than
/// stamped with today, which would file an empty January under August.
fn period(statement: &Element, lines: &[ParsedLine]) -> Result<(Date, Date)> {
    // Both ends or neither: a `FrToDt` that states only one of them tells us
    // less than the entries do, so it falls through rather than half-answering.
    if let Some(range) = statement.at(&["FrToDt"]) {
        let (stated_from, stated_to) = (range.text_at(&["FrDtTm"]), range.text_at(&["ToDtTm"]));
        if !stated_from.is_empty() && !stated_to.is_empty() {
            let from = day("the statement period's start", stated_from)?;
            let to = day("the statement period's end", stated_to)?;
            return Ok((from, to));
        }
    }
    let mut booked: Vec<Date> = lines.iter().map(|line| line.booked_on).collect();
    booked.sort_unstable();
    match (booked.first(), booked.last()) {
        (Some(from), Some(to)) => Ok((*from, *to)),
        _ => Err(StoreError::Validation(
            "this statement states neither a period (FrToDt) nor a single entry, so there is \
             nothing in it to import"
                .to_owned(),
        )),
    }
}

/// The balance of one of the named types, signed, or `None` when the statement
/// states none of them.
///
/// The codes are tried in order, so `OPBD` wins over `PRCD` in a file that
/// carries both.
fn balance(statement: &Element, codes: &[&str]) -> Result<Option<i64>> {
    for code in codes {
        for candidate in statement.children_named("Bal") {
            let stated = candidate.text_at(&["Tp", "CdOrPrtry", "Cd"]);
            if !stated.eq_ignore_ascii_case(code) {
                continue;
            }
            let amount = decimal_amount("the statement balance", candidate.text_at(&["Amt"]))?;
            return Ok(Some(signed(
                amount,
                candidate.text_at(&["CdtDbtInd"]),
                false,
                "the statement balance",
            )?));
        }
    }
    Ok(None)
}

/// Reads one `<Ntry>` into a line. `at` is its 1-based position, and it is what
/// every refusal names; `owner` is the account holder's own name as the
/// statement states it, which is how a fallback avoids reading them back.
fn read_entry(at: usize, entry: &Element, owner: &str) -> Result<ParsedLine> {
    let term = format!("entry {at} of this statement");
    let amount = decimal_amount(&term, entry.text_at(&["Amt"]))?;
    // `RvslInd` is how a bank says "this entry undoes one": a reversed credit
    // is money leaving, whatever the indicator says.
    let reversal = entry.text_at(&["RvslInd"]).eq_ignore_ascii_case("true");
    let amount_cents = signed(amount, entry.text_at(&["CdtDbtInd"]), reversal, &term)?;

    let booked_on = day(&format!("{term}: the booking date"), booking_date(entry))?;
    let value_on = match value_date(entry) {
        "" => booked_on,
        stated => day(&format!("{term}: the value date"), stated)?,
    };

    // One entry, one movement of money — even when the bank details several
    // transactions inside it. A single `TxDtls` is the ordinary case and names
    // the counterparty; a batch names none, because none of the several is the
    // counterparty of the entry.
    let details: Vec<&Element> = entry
        .at(&["NtryDtls"])
        .map(|d| d.children_named("TxDtls").collect())
        .unwrap_or_default();
    let single = match details.as_slice() {
        [only] => Some(*only),
        _ => None,
    };

    let (counterparty_name, counterparty_iban) = single
        .and_then(|tx| counterparty(tx, amount_cents > 0, owner))
        .unwrap_or_default();
    Ok(ParsedLine {
        booked_on,
        value_on,
        amount_cents,
        currency: currency_of(entry, single),
        counterparty_name,
        counterparty_iban,
        remittance: remittance(entry, single),
        bank_ref: bank_reference(entry, single),
    })
}

/// Applies CAMT's sign convention: a credit is money in, a debit is money out,
/// and a reversal turns either around.
///
/// # Errors
/// [`StoreError::Validation`] when the entry states no indicator — the one
/// thing about a bank line that cannot be guessed, because guessing it wrong
/// reverses the direction of money.
fn signed(amount: i64, indicator: &str, reversal: bool, term: &str) -> Result<i64> {
    let inbound = if indicator.eq_ignore_ascii_case("CRDT") {
        true
    } else if indicator.eq_ignore_ascii_case("DBIT") {
        false
    } else {
        return Err(StoreError::Validation(format!(
            "{term} does not say whether the money came in or went out (CdtDbtInd), and that is \
             not something to guess at"
        )));
    };
    let magnitude = amount.abs();
    Ok(if inbound != reversal {
        magnitude
    } else {
        -magnitude
    })
}

/// The entry's booking date, in whichever of the two spellings the bank used.
fn booking_date(entry: &Element) -> &str {
    let dated = entry.text_at(&["BookgDt", "Dt"]);
    if dated.is_empty() {
        entry.text_at(&["BookgDt", "DtTm"])
    } else {
        dated
    }
}

/// The entry's value date, or `""` when it states none.
fn value_date(entry: &Element) -> &str {
    let dated = entry.text_at(&["ValDt", "Dt"]);
    if dated.is_empty() {
        entry.text_at(&["ValDt", "DtTm"])
    } else {
        dated
    }
}

/// Reads a day from either an ISO date (`2026-01-05`) or the dateTime a bank
/// writes where the schema asks for a date (`2026-01-05T00:00:00`,
/// `2026-01-05+01:00`).
///
/// The time of day is dropped rather than converted: a booking date is a
/// calendar fact in the bank's own jurisdiction, and shifting it into UTC would
/// move a late-evening payment into the next day for no gain.
fn day(term: &str, raw: &str) -> Result<Date> {
    let text = raw.trim();
    let text = text.split('T').next().unwrap_or(text).trim();
    let text = if text.len() > 10 {
        text.get(..10).unwrap_or(text)
    } else {
        text
    };
    date(term, text)
}

/// The line's currency: the one stated on the entry's own amount, falling back
/// to the transaction's.
fn currency_of(entry: &Element, single: Option<&Element>) -> String {
    let stated = entry
        .at(&["Amt"])
        .map(|amount| amount.attr("Ccy"))
        .unwrap_or_default();
    if !stated.trim().is_empty() {
        return stated.trim().to_uppercase();
    }
    single
        .and_then(|tx| tx.at(&["Amt"]))
        .map(|amount| amount.attr("Ccy").trim().to_uppercase())
        .unwrap_or_default()
}

/// Who the other party was: the debtor on money in, the creditor on money out.
///
/// Returns their name and IBAN, or `None` when the transaction names neither —
/// a bank charge has no counterparty but the bank itself.
///
/// # The fallback, and why it is guarded
///
/// The direction-implied role is right for an ordinary transaction. A
/// **reversal** is where banks disagree: some restate the parties in the
/// original instruction's roles (the returning customer stays the `Dbtr` of the
/// credit that was undone), others swap them to match the money's new
/// direction. Reading only one convention leaves the counterparty blank on
/// exactly the lines a bookkeeper most needs to identify.
///
/// So the other role is tried second — but only when the statement names the
/// **account holder** and the fallback is not them. A file states both roles
/// and one of them is always us; an unguarded fallback would fill the
/// reconciliation screen with the tenant's own name, which looks like data and
/// is not. With no owner stated there is no guard, so there is no fallback:
/// blank is the honest answer.
fn counterparty(tx: &Element, credit: bool, owner: &str) -> Option<(String, String)> {
    let parties = tx.at(&["RltdPties"])?;
    let ordered = if credit {
        [("Dbtr", "DbtrAcct"), ("Cdtr", "CdtrAcct")]
    } else {
        [("Cdtr", "CdtrAcct"), ("Dbtr", "DbtrAcct")]
    };
    for (index, (role, account)) in ordered.into_iter().enumerate() {
        let name = party_name(parties, role);
        let iban = parties.text_at(&[account, "Id", "IBAN"]).to_owned();
        if name.is_empty() && iban.is_empty() {
            continue;
        }
        let fallback = index > 0;
        if fallback && (owner.trim().is_empty() || same_party(&name, owner)) {
            continue;
        }
        return Some((name, iban));
    }
    None
}

/// A party's name: `Nm`, or — for a company that states itself structurally —
/// the trading name underneath it.
fn party_name(parties: &Element, role: &str) -> String {
    let Some(party) = parties.at(&[role]) else {
        return String::new();
    };
    let stated = party.text_at(&["Nm"]);
    if stated.is_empty() {
        party.text_at(&["Pty", "Nm"]).to_owned()
    } else {
        stated.to_owned()
    }
}

/// Whether two names are the same party, allowing for the spacing and case a
/// bank varies between the account header and a transaction.
fn same_party(one: &str, other: &str) -> bool {
    let squash = |value: &str| {
        value
            .split_whitespace()
            .map(str::to_lowercase)
            .collect::<Vec<_>>()
            .join(" ")
    };
    !one.trim().is_empty() && squash(one) == squash(other)
}

/// What was written on the payment.
///
/// Unstructured lines first, in the order the file states them, then the
/// structured creditor reference — the one an invoice's number travels in when
/// a payer's bank offers the field. Both, because banks differ about which one
/// a payer's text ends up in, and B4.09 searches this field for our own invoice
/// numbers. A batched entry has no transaction to read, so it falls back to the
/// entry's own additional information, which is where a bank names a payroll
/// run or a collection.
fn remittance(entry: &Element, single: Option<&Element>) -> String {
    let mut parts: Vec<String> = Vec::new();
    if let Some(info) = single.and_then(|tx| tx.at(&["RmtInf"])) {
        for line in info.children_named("Ustrd") {
            if !line.text.is_empty() {
                parts.push(line.text.clone());
            }
        }
        for structured in info.children_named("Strd") {
            let reference = structured.text_at(&["CdtrRefInf", "Ref"]);
            if !reference.is_empty() {
                parts.push(reference.to_owned());
            }
        }
    }
    if parts.is_empty() {
        let additional = entry.text_at(&["AddtlNtryInf"]);
        if !additional.is_empty() {
            parts.push(additional.to_owned());
        }
    }
    parts.join(" ")
}

/// The bank's own reference for the entry.
///
/// `AcctSvcrRef` is the servicing bank's identifier and the most stable of the
/// three; `NtryRef` is what some banks state instead; the payer's `EndToEndId`
/// is the last resort, and deliberately last, because a payer choosing
/// `NOTPROVIDED` (which the standard allows) would otherwise make every
/// transaction of a busy day look like the same one.
fn bank_reference(entry: &Element, single: Option<&Element>) -> String {
    for candidate in [
        entry.text_at(&["AcctSvcrRef"]),
        entry.text_at(&["NtryRef"]),
        single.map_or("", |tx| tx.text_at(&["Refs", "AcctSvcrRef"])),
        single.map_or("", |tx| tx.text_at(&["Refs", "EndToEndId"])),
    ] {
        let candidate = candidate.trim();
        if !candidate.is_empty() && !candidate.eq_ignore_ascii_case("NOTPROVIDED") {
            return candidate.to_owned();
        }
    }
    String::new()
}

#[cfg(test)]
mod tests {
    use super::*;
    use time::Month;

    /// A minimal but complete CAMT.053, with `body` inserted between the
    /// account and the closing tags — the frame every case below varies.
    fn document(body: &str) -> String {
        format!(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<Document xmlns="urn:iso:std:iso:20022:tech:xsd:camt.053.001.02">
  <BkToCstmrStmt>
    <GrpHdr><MsgId>MSG-1</MsgId><CreDtTm>2026-02-01T02:00:00</CreDtTm></GrpHdr>
    <Stmt>
      <Id>2026/001</Id>
      <CreDtTm>2026-02-01T02:00:00</CreDtTm>
      <FrToDt><FrDtTm>2026-01-01T00:00:00</FrDtTm><ToDtTm>2026-01-31T23:59:59</ToDtTm></FrToDt>
      <Acct><Id><IBAN>DE02120300000000202051</IBAN></Id><Ccy>EUR</Ccy>
        <Ownr><Nm>Our Own Company BV</Nm></Ownr></Acct>
      {body}
    </Stmt>
  </BkToCstmrStmt>
</Document>"#
        )
    }

    fn day_of(year: i32, month: Month, day: u8) -> Date {
        Date::from_calendar_date(year, month, day).unwrap_or(Date::MIN)
    }

    fn refused(xml: &str) -> String {
        match parse_camt053(xml.as_bytes()) {
            Err(StoreError::Validation(message)) => message,
            other => panic!("expected a Validation refusal, got {other:?}"),
        }
    }

    fn parsed(xml: &str) -> ParsedStatement {
        match parse_camt053(xml.as_bytes()) {
            Ok(statement) => statement,
            other => panic!("expected a statement, got {other:?}"),
        }
    }

    const CREDIT: &str = r#"
      <Ntry>
        <Amt Ccy="EUR">1250.00</Amt>
        <CdtDbtInd>CRDT</CdtDbtInd>
        <Sts>BOOK</Sts>
        <BookgDt><Dt>2026-01-05</Dt></BookgDt>
        <ValDt><Dt>2026-01-04</Dt></ValDt>
        <AcctSvcrRef>REF-0001</AcctSvcrRef>
        <NtryDtls><TxDtls>
          <Refs><EndToEndId>E2E-1</EndToEndId></Refs>
          <RltdPties>
            <Dbtr><Nm>Kaffeehaus Berlin GmbH</Nm></Dbtr>
            <DbtrAcct><Id><IBAN>NL91ABNA0417164300</IBAN></Id></DbtrAcct>
            <Cdtr><Nm>Our Own Company BV</Nm></Cdtr>
            <CdtrAcct><Id><IBAN>DE02120300000000202051</IBAN></Id></CdtrAcct>
          </RltdPties>
          <RmtInf><Ustrd>Rechnung INV-2026-00007</Ustrd></RmtInf>
        </TxDtls></NtryDtls>
      </Ntry>"#;

    const DEBIT: &str = r#"
      <Ntry>
        <Amt Ccy="EUR">89.90</Amt>
        <CdtDbtInd>DBIT</CdtDbtInd>
        <BookgDt><Dt>2026-01-07</Dt></BookgDt>
        <NtryDtls><TxDtls>
          <RltdPties>
            <Dbtr><Nm>Our Own Company BV</Nm></Dbtr>
            <Cdtr><Nm>Stadtwerke</Nm></Cdtr>
            <CdtrAcct><Id><IBAN>DE89370400440532013000</IBAN></Id></CdtrAcct>
          </RltdPties>
          <RmtInf><Strd><CdtrRefInf><Ref>RF18539007547034</Ref></CdtrRefInf></Strd></RmtInf>
        </TxDtls></NtryDtls>
      </Ntry>"#;

    #[test]
    fn a_credit_is_money_in_and_its_counterparty_is_the_payer() {
        let statement = parsed(&document(CREDIT));
        assert_eq!(statement.source, BankSource::Camt);
        assert_eq!(statement.account_iban, "DE02120300000000202051");
        assert_eq!(statement.currency, "EUR");
        assert_eq!(statement.statement_ref, "2026/001");
        assert_eq!(statement.from_date, day_of(2026, Month::January, 1));
        assert_eq!(statement.to_date, day_of(2026, Month::January, 31));

        let line = &statement.lines[0];
        assert_eq!(line.amount_cents, 125_000, "a credit is positive");
        assert_eq!(line.booked_on, day_of(2026, Month::January, 5));
        assert_eq!(line.value_on, day_of(2026, Month::January, 4));
        assert_eq!(
            line.counterparty_name, "Kaffeehaus Berlin GmbH",
            "on money in the counterparty is the debtor, never the account holder"
        );
        assert_eq!(line.counterparty_iban, "NL91ABNA0417164300");
        assert_eq!(line.remittance, "Rechnung INV-2026-00007");
        assert_eq!(line.bank_ref, "REF-0001");
    }

    #[test]
    fn a_debit_is_money_out_and_its_counterparty_is_the_payee() {
        let statement = parsed(&document(DEBIT));
        let line = &statement.lines[0];
        assert_eq!(line.amount_cents, -8_990, "a debit is negative");
        assert_eq!(
            line.value_on, line.booked_on,
            "a value date the file omits is the booking date, not nothing"
        );
        assert_eq!(line.counterparty_name, "Stadtwerke");
        assert_eq!(line.counterparty_iban, "DE89370400440532013000");
        assert_eq!(
            line.remittance, "RF18539007547034",
            "a structured creditor reference is remittance too"
        );
    }

    #[test]
    fn a_reversal_turns_the_direction_around() {
        let reversed = r#"
      <Ntry>
        <Amt Ccy="EUR">1250.00</Amt>
        <CdtDbtInd>CRDT</CdtDbtInd>
        <RvslInd>true</RvslInd>
        <BookgDt><Dt>2026-01-09</Dt></BookgDt>
        <AddtlNtryInf>Rueckbuchung</AddtlNtryInf>
      </Ntry>"#;
        let line = &parsed(&document(reversed)).lines[0];
        assert_eq!(
            line.amount_cents, -125_000,
            "a reversed credit is money leaving"
        );
        assert_eq!(
            line.remittance, "Rueckbuchung",
            "an entry with no transaction details falls back to its own note"
        );
    }

    #[test]
    fn a_reversal_that_keeps_the_original_roles_still_names_the_other_party() {
        // Money out, but the file states only the party of the credit it
        // undoes — the convention half the banks follow. The other role is
        // read, because the statement names the account holder and it is not
        // them.
        let returned = r#"
      <Ntry>
        <Amt Ccy="EUR">1250.00</Amt>
        <CdtDbtInd>CRDT</CdtDbtInd>
        <RvslInd>true</RvslInd>
        <BookgDt><Dt>2026-01-09</Dt></BookgDt>
        <NtryDtls><TxDtls><RltdPties>
          <Dbtr><Nm>Kaffeehaus Berlin GmbH</Nm></Dbtr>
          <DbtrAcct><Id><IBAN>NL91ABNA0417164300</IBAN></Id></DbtrAcct>
        </RltdPties></TxDtls></NtryDtls>
      </Ntry>"#;
        let line = &parsed(&document(returned)).lines[0];
        assert_eq!(line.amount_cents, -125_000);
        assert_eq!(line.counterparty_name, "Kaffeehaus Berlin GmbH");
        assert_eq!(line.counterparty_iban, "NL91ABNA0417164300");

        // But the fallback never reads the account holder back at them: here
        // the only other role is us, so the line has no counterparty.
        let ourselves = r#"
      <Ntry>
        <Amt Ccy="EUR">40.00</Amt>
        <CdtDbtInd>DBIT</CdtDbtInd>
        <BookgDt><Dt>2026-01-09</Dt></BookgDt>
        <NtryDtls><TxDtls><RltdPties>
          <Dbtr><Nm>our own   company bv</Nm></Dbtr>
        </RltdPties></TxDtls></NtryDtls>
      </Ntry>"#;
        assert_eq!(parsed(&document(ourselves)).lines[0].counterparty_name, "");

        // And with no owner stated there is no guard, so there is no fallback.
        let unguarded = document(ourselves).replace("<Ownr><Nm>Our Own Company BV</Nm></Ownr>", "");
        assert_eq!(parsed(&unguarded).lines[0].counterparty_name, "");
    }

    #[test]
    fn a_batch_is_one_line_with_no_invented_counterparty() {
        let batch = r#"
      <Ntry>
        <Amt Ccy="EUR">4500.00</Amt>
        <CdtDbtInd>DBIT</CdtDbtInd>
        <BookgDt><Dt>2026-01-28</Dt></BookgDt>
        <AddtlNtryInf>Gehaltslauf Januar</AddtlNtryInf>
        <NtryDtls>
          <TxDtls><RltdPties><Cdtr><Nm>A Person</Nm></Cdtr></RltdPties>
            <RmtInf><Ustrd>Gehalt</Ustrd></RmtInf></TxDtls>
          <TxDtls><RltdPties><Cdtr><Nm>Another Person</Nm></Cdtr></RltdPties>
            <RmtInf><Ustrd>Gehalt</Ustrd></RmtInf></TxDtls>
        </NtryDtls>
      </Ntry>"#;
        let statement = parsed(&document(batch));
        assert_eq!(statement.lines.len(), 1, "the bank moved money once");
        let line = &statement.lines[0];
        assert_eq!(line.amount_cents, -450_000);
        assert_eq!(
            line.counterparty_name, "",
            "neither of the several is the counterparty of the entry"
        );
        assert_eq!(line.remittance, "Gehaltslauf Januar");
    }

    #[test]
    fn an_entry_that_is_not_booked_yet_is_counted_and_skipped() {
        let pending = format!(
            "{CREDIT}{}",
            r#"
      <Ntry>
        <Amt Ccy="EUR">10.00</Amt>
        <CdtDbtInd>DBIT</CdtDbtInd>
        <Sts><Cd>PDNG</Cd></Sts>
        <BookgDt><Dt>2026-01-30</Dt></BookgDt>
      </Ntry>"#
        );
        let statement = parsed(&document(&pending));
        assert_eq!(statement.lines.len(), 1);
        assert_eq!(statement.unbooked, 1, "a pending item is not reconcilable");
    }

    #[test]
    fn the_balances_are_signed_and_prcd_stands_in_for_a_missing_opbd() {
        let balances = r#"
      <Bal>
        <Tp><CdOrPrtry><Cd>PRCD</Cd></CdOrPrtry></Tp>
        <Amt Ccy="EUR">1000.00</Amt><CdtDbtInd>CRDT</CdtDbtInd>
        <Dt><Dt>2026-01-01</Dt></Dt>
      </Bal>
      <Bal>
        <Tp><CdOrPrtry><Cd>CLBD</Cd></CdOrPrtry></Tp>
        <Amt Ccy="EUR">250.50</Amt><CdtDbtInd>DBIT</CdtDbtInd>
        <Dt><Dt>2026-01-31</Dt></Dt>
      </Bal>"#;
        let statement = parsed(&document(balances));
        assert_eq!(statement.opening_balance_cents, Some(100_000));
        assert_eq!(
            statement.closing_balance_cents,
            Some(-25_050),
            "an overdrawn account is a debit balance"
        );

        let silent = parsed(&document(CREDIT));
        assert_eq!(
            (silent.opening_balance_cents, silent.closing_balance_cents),
            (None, None),
            "a balance the file did not state is absent, not zero"
        );
    }

    #[test]
    fn a_period_the_file_omits_is_the_span_of_its_entries() {
        let frame = document(&format!("{CREDIT}{DEBIT}"));
        let without = frame.replace(
            "<FrToDt><FrDtTm>2026-01-01T00:00:00</FrDtTm><ToDtTm>2026-01-31T23:59:59</ToDtTm></FrToDt>",
            "",
        );
        let statement = parsed(&without);
        assert_eq!(statement.from_date, day_of(2026, Month::January, 5));
        assert_eq!(statement.to_date, day_of(2026, Month::January, 7));
    }

    #[test]
    fn a_date_written_as_a_datetime_is_the_same_day() {
        assert_eq!(
            day("t", "2026-01-05T23:30:00").ok(),
            Some(day_of(2026, Month::January, 5))
        );
        assert_eq!(
            day("t", "2026-01-05+01:00").ok(),
            Some(day_of(2026, Month::January, 5))
        );
        assert_eq!(
            day("t", "2026-01-05").ok(),
            Some(day_of(2026, Month::January, 5))
        );
        assert!(day("t", "the fifth").is_err());
    }

    #[test]
    fn an_entry_that_does_not_say_which_way_the_money_went_is_refused_by_number() {
        let no_indicator = format!(
            "{CREDIT}{}",
            r#"
      <Ntry>
        <Amt Ccy="EUR">10.00</Amt>
        <BookgDt><Dt>2026-01-30</Dt></BookgDt>
      </Ntry>"#
        );
        let message = refused(&document(&no_indicator));
        assert!(message.contains("entry 2"), "names the entry: {message}");
        assert!(message.contains("CdtDbtInd"));
    }

    #[test]
    fn an_unreadable_amount_or_date_is_refused_by_number() {
        let bad_amount = r#"
      <Ntry><Amt Ccy="EUR">1.005</Amt><CdtDbtInd>CRDT</CdtDbtInd>
        <BookgDt><Dt>2026-01-05</Dt></BookgDt></Ntry>"#;
        assert!(refused(&document(bad_amount)).contains("entry 1"));

        let bad_date = r#"
      <Ntry><Amt Ccy="EUR">1.00</Amt><CdtDbtInd>CRDT</CdtDbtInd>
        <BookgDt><Dt>05.01.2026</Dt></BookgDt></Ntry>"#;
        let message = refused(&document(bad_date));
        assert!(message.contains("entry 1") && message.contains("booking date"));
    }

    #[test]
    fn a_file_that_is_not_a_statement_says_so() {
        assert!(refused("<Document><Foo/></Document>").contains("CAMT.053"));
        assert!(refused("<Invoice/>").contains("CAMT.053"));
        assert!(
            refused("<Document><BkToCstmrStmt><GrpHdr/></BkToCstmrStmt></Document>")
                .contains("carries no statement")
        );

        let two = document(CREDIT).replace(
            "</Stmt>",
            "</Stmt><Stmt><Id>2026/002</Id><Acct><Id><IBAN>NL91ABNA0417164300</IBAN></Id></Acct></Stmt>",
        );
        assert!(refused(&two).contains("one at a time"));

        match parse_camt053(b"PK\x03\x04 and then a zip") {
            Err(StoreError::Validation(message)) => assert!(message.contains("ZIP")),
            other => panic!("expected the ZIP answer, got {other:?}"),
        }
        match parse_camt053(&[0xFF, 0xFE, 0x00]) {
            Err(StoreError::Validation(message)) => assert!(message.contains("UTF-8")),
            other => panic!("expected the encoding answer, got {other:?}"),
        }
    }

    #[test]
    fn a_prefixed_document_reads_the_same_as_an_unprefixed_one() {
        // Two banks writing the same standard routinely choose different
        // prefixes for the same namespace; `billing_xml_tree` strips them, and
        // this is the proof the statement reader benefits.
        let prefixed = r#"<?xml version="1.0" encoding="UTF-8"?>
<ns2:Document xmlns:ns2="urn:iso:std:iso:20022:tech:xsd:camt.053.001.08">
  <ns2:BkToCstmrStmt>
    <ns2:Stmt>
      <ns2:Id>2026/001</ns2:Id>
      <ns2:Acct><ns2:Id><ns2:IBAN>DE02120300000000202051</ns2:IBAN></ns2:Id>
        <ns2:Ccy>EUR</ns2:Ccy></ns2:Acct>
      <ns2:Ntry>
        <ns2:Amt Ccy="EUR">1250.00</ns2:Amt>
        <ns2:CdtDbtInd>CRDT</ns2:CdtDbtInd>
        <ns2:BookgDt><ns2:Dt>2026-01-05</ns2:Dt></ns2:BookgDt>
        <ns2:ValDt><ns2:Dt>2026-01-04</ns2:Dt></ns2:ValDt>
        <ns2:AcctSvcrRef>REF-0001</ns2:AcctSvcrRef>
        <ns2:NtryDtls><ns2:TxDtls>
          <ns2:RltdPties>
            <ns2:Dbtr><ns2:Nm>Kaffeehaus Berlin GmbH</ns2:Nm></ns2:Dbtr>
            <ns2:DbtrAcct><ns2:Id><ns2:IBAN>NL91ABNA0417164300</ns2:IBAN></ns2:Id></ns2:DbtrAcct>
          </ns2:RltdPties>
          <ns2:RmtInf><ns2:Ustrd>Rechnung INV-2026-00007</ns2:Ustrd></ns2:RmtInf>
        </ns2:TxDtls></ns2:NtryDtls>
      </ns2:Ntry>
    </ns2:Stmt>
  </ns2:BkToCstmrStmt>
</ns2:Document>"#;
        assert_eq!(parsed(&document(CREDIT)).lines, parsed(prefixed).lines);
    }

    #[test]
    fn a_bank_that_states_no_account_currency_takes_it_from_the_entries() {
        let frame = document(CREDIT).replace("<Ccy>EUR</Ccy>", "");
        assert_eq!(parsed(&frame).currency, "EUR");

        let currencyless = document(CREDIT)
            .replace("<Ccy>EUR</Ccy>", "")
            .replace(r#"<Amt Ccy="EUR">"#, "<Amt>");
        assert!(refused(&currencyless).contains("names no currency"));
    }
}
