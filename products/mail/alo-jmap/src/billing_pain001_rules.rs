//! What a `pain.001` message has to satisfy before a bank will execute it
//! (alo Billing, wave B2.12) — the schema subset and the EPC scheme rules,
//! checked over the document we actually emit.
//!
//! The sibling of [`crate::billing_einvoice_rules`], and it exists for the same
//! reason and with the same honesty about what it is not.
//!
//! **What it is.** A checker over the *rendered bytes*: the file is parsed back
//! with the store's defensive reader ([`alo_store::billing_xml_tree`]) and held
//! against the part of `pain.001` we write — every element the schema requires,
//! in the order the schema sequences them, with the data types and lengths ISO
//! 20022 gives them, plus the narrowings the EPC's customer-to-bank guidelines
//! add for SEPA credit transfers (euro only, `SLEV`, `SEPA` service level, the
//! basic Latin character set, `Max35Text` identifiers, `Max70Text` names).
//! Checking the output rather than the model is the point: a misordered element
//! or a name a bank cannot spell is invisible in the model and fatal in the
//! file.
//!
//! **What it is not.** It is not the XSD. Running the normative schema means an
//! XML Schema processor and a downloaded artefact in a public repository, which
//! `CLAUDE.md` rules out for the same reason the e-invoice schematron is ruled
//! out. So this is a hand-written subset, cited by element path, and validating
//! our own two golden files against the real XSD once, offline, stays an open
//! item for a human (`docs/autonomy/STATE.md`). What this checker does is make
//! that a one-off confirmation rather than a standing risk: it fails on every
//! change that would break the file, in the same test run as the goldens.

use alo_store::billing_xml_tree::{Element, parse};

use crate::billing_einvoice_rules::Violation;
use crate::billing_pain001::{Pain001Version, SEPA_NAME_MAX_CHARS};

/// The largest amount one SEPA credit transfer may carry, in cents
/// (€999 999 999.99) — the scheme's own ceiling.
const MAX_TRANSFER_CENTS: i64 = 99_999_999_999;

/// `Max35Text`, the length every identifier in the message is held to.
const ID_MAX: usize = 35;

/// `Max140Text`, the length of one unstructured remittance line.
const REMITTANCE_MAX: usize = 140;

/// Every rule `xml` breaks, read as a `pain.001` of `version`.
///
/// An empty result means the file is one our reading of the standard accepts;
/// see the module documentation for what that reading covers.
#[must_use]
pub fn violations(xml: &str, version: Pain001Version) -> Vec<Violation> {
    let mut found = Vec::new();
    if !xml.contains(&format!("xmlns=\"{}\"", version.namespace())) {
        found.push(Violation::new(
            "Document",
            format!(
                "the document does not declare the {} namespace",
                version.as_str()
            ),
        ));
    }
    let root = match parse(xml) {
        Ok(root) => root,
        Err(error) => {
            found.push(Violation::new(
                "Document",
                format!("the file is not well-formed XML ({error})"),
            ));
            return found;
        }
    };
    if root.name != "Document" {
        found.push(Violation::new(
            "Document",
            "the root element is not Document",
        ));
        return found;
    }
    let Some(body) = root.child("CstmrCdtTrfInitn") else {
        found.push(Violation::new(
            "Document/CstmrCdtTrfInitn",
            "the message body is missing",
        ));
        return found;
    };
    sequence(body, "CstmrCdtTrfInitn", &["GrpHdr", "PmtInf"], &mut found);

    let blocks: Vec<&Element> = body.children_named("PmtInf").collect();
    if blocks.is_empty() {
        found.push(Violation::new(
            "CstmrCdtTrfInitn/PmtInf",
            "the message instructs no payments",
        ));
    }
    let transfers: Vec<&Element> = blocks
        .iter()
        .flat_map(|block| block.children_named("CdtTrfTxInf"))
        .collect();

    group_header(body, &transfers, &mut found);
    for block in &blocks {
        payment_information(block, version, &mut found);
    }
    found
}

/// `GrpHdr` — the header, and the two figures the bank checks the whole file
/// against.
fn group_header(body: &Element, transfers: &[&Element], found: &mut Vec<Violation>) {
    let Some(header) = body.child("GrpHdr") else {
        found.push(Violation::new("GrpHdr", "the group header is missing"));
        return;
    };
    sequence(
        header,
        "GrpHdr",
        &["MsgId", "CreDtTm", "NbOfTxs", "CtrlSum", "InitgPty"],
        found,
    );
    identifier("GrpHdr/MsgId", header.text_at(&["MsgId"]), found);
    if !is_timestamp(header.text_at(&["CreDtTm"])) {
        found.push(Violation::new(
            "GrpHdr/CreDtTm",
            "the creation moment is not an ISODateTime (YYYY-MM-DDThh:mm:ss)",
        ));
    }
    text(
        "GrpHdr/InitgPty/Nm",
        header.text_at(&["InitgPty", "Nm"]),
        SEPA_NAME_MAX_CHARS,
        found,
    );
    counts(header, "GrpHdr", transfers, found);
}

/// `PmtInf` — one debtor account and one execution date, and every transfer
/// made from it.
fn payment_information(block: &Element, version: Pain001Version, found: &mut Vec<Violation>) {
    sequence(
        block,
        "PmtInf",
        &[
            "PmtInfId",
            "PmtMtd",
            "BtchBookg",
            "NbOfTxs",
            "CtrlSum",
            "PmtTpInf",
            "ReqdExctnDt",
            "Dbtr",
            "DbtrAcct",
            "DbtrAgt",
            "ChrgBr",
            "CdtTrfTxInf",
        ],
        found,
    );
    identifier("PmtInf/PmtInfId", block.text_at(&["PmtInfId"]), found);
    fixed("PmtInf/PmtMtd", block.text_at(&["PmtMtd"]), "TRF", found);
    let batch = block.text_at(&["BtchBookg"]);
    if !batch.is_empty() && batch != "true" && batch != "false" {
        found.push(Violation::new(
            "PmtInf/BtchBookg",
            "the batch-booking flag is neither true nor false",
        ));
    }
    fixed(
        "PmtInf/PmtTpInf/SvcLvl/Cd",
        block.text_at(&["PmtTpInf", "SvcLvl", "Cd"]),
        "SEPA",
        found,
    );
    // The one element the two versions spell differently: a date in `.03`, a
    // choice wrapping a date in `.09`.
    let execution = match version {
        Pain001Version::V03 => block.text_at(&["ReqdExctnDt"]).to_owned(),
        Pain001Version::V09 => block.text_at(&["ReqdExctnDt", "Dt"]).to_owned(),
    };
    if !is_date(&execution) {
        found.push(Violation::new(
            "PmtInf/ReqdExctnDt",
            "the requested execution date is not an ISODate (YYYY-MM-DD)",
        ));
    }
    text(
        "PmtInf/Dbtr/Nm",
        block.text_at(&["Dbtr", "Nm"]),
        SEPA_NAME_MAX_CHARS,
        found,
    );
    iban("PmtInf/DbtrAcct", block.at(&["DbtrAcct", "Id"]), found);
    agent("PmtInf/DbtrAgt", block.child("DbtrAgt"), version, found);
    fixed("PmtInf/ChrgBr", block.text_at(&["ChrgBr"]), "SLEV", found);

    let transfers: Vec<&Element> = block.children_named("CdtTrfTxInf").collect();
    if transfers.is_empty() {
        found.push(Violation::new(
            "PmtInf/CdtTrfTxInf",
            "the payment block instructs no transfers",
        ));
    }
    counts(block, "PmtInf", &transfers, found);
    for transfer in &transfers {
        credit_transfer(transfer, version, found);
    }
}

/// One `CdtTrfTxInf` — a payment to one supplier.
fn credit_transfer(transfer: &Element, version: Pain001Version, found: &mut Vec<Violation>) {
    sequence(
        transfer,
        "CdtTrfTxInf",
        &["PmtId", "Amt", "CdtrAgt", "Cdtr", "CdtrAcct", "RmtInf"],
        found,
    );
    identifier(
        "CdtTrfTxInf/PmtId/EndToEndId",
        transfer.text_at(&["PmtId", "EndToEndId"]),
        found,
    );
    match transfer.at(&["Amt", "InstdAmt"]) {
        None => found.push(Violation::new(
            "CdtTrfTxInf/Amt/InstdAmt",
            "the amount to transfer is missing",
        )),
        Some(instructed) => {
            if instructed.attr("Ccy") != "EUR" {
                found.push(Violation::new(
                    "CdtTrfTxInf/Amt/InstdAmt",
                    "a SEPA credit transfer is in euro; the amount states another currency",
                ));
            }
            match cents(&instructed.text) {
                None => found.push(Violation::new(
                    "CdtTrfTxInf/Amt/InstdAmt",
                    "the amount is not a decimal with exactly two fraction digits",
                )),
                Some(value) if value <= 0 => found.push(Violation::new(
                    "CdtTrfTxInf/Amt/InstdAmt",
                    "a transfer moves a positive amount",
                )),
                Some(value) if value > MAX_TRANSFER_CENTS => found.push(Violation::new(
                    "CdtTrfTxInf/Amt/InstdAmt",
                    "the amount is above the scheme's ceiling of 999999999.99",
                )),
                Some(_) => {}
            }
        }
    }
    if let Some(bank) = transfer.child("CdtrAgt") {
        agent("CdtTrfTxInf/CdtrAgt", Some(bank), version, found);
    }
    text(
        "CdtTrfTxInf/Cdtr/Nm",
        transfer.text_at(&["Cdtr", "Nm"]),
        SEPA_NAME_MAX_CHARS,
        found,
    );
    iban(
        "CdtTrfTxInf/CdtrAcct",
        transfer.at(&["CdtrAcct", "Id"]),
        found,
    );
    if let Some(remittance) = transfer.child("RmtInf") {
        let lines = remittance.children_named("Ustrd").count();
        if lines > 1 {
            found.push(Violation::new(
                "CdtTrfTxInf/RmtInf/Ustrd",
                "the scheme carries one unstructured remittance line, not several",
            ));
        }
        text(
            "CdtTrfTxInf/RmtInf/Ustrd",
            remittance.text_at(&["Ustrd"]),
            REMITTANCE_MAX,
            found,
        );
    }
}

// ---- the shapes --------------------------------------------------------------

/// Checks that `element`'s children are all expected and appear in the schema's
/// order. A sequence is not a set: an element in the wrong place is refused by
/// a validating parser exactly as a missing one is.
fn sequence(
    element: &Element,
    path: &'static str,
    order: &[&'static str],
    found: &mut Vec<Violation>,
) {
    let mut highest = 0usize;
    for child in &element.children {
        match order.iter().position(|name| *name == child.name) {
            None => found.push(Violation::new(
                path,
                format!("{} carries an element the schema does not have here", path),
            )),
            Some(at) => {
                if at < highest {
                    found.push(Violation::new(
                        path,
                        format!("the elements of {path} are not in the schema's order"),
                    ));
                }
                highest = highest.max(at);
            }
        }
    }
}

/// Checks the two figures a block states about itself against the transfers it
/// actually carries — the file's own internal proof, and the first thing a bank
/// rejects a file for.
fn counts(
    element: &Element,
    path: &'static str,
    transfers: &[&Element],
    found: &mut Vec<Violation>,
) {
    let stated: Option<usize> = element.text_at(&["NbOfTxs"]).parse().ok();
    if stated != Some(transfers.len()) {
        found.push(Violation::new(
            path,
            format!("{path}/NbOfTxs does not state how many transfers the file carries"),
        ));
    }
    let summed: Option<i64> = transfers
        .iter()
        .map(|transfer| {
            transfer
                .at(&["Amt", "InstdAmt"])
                .and_then(|amount| cents(&amount.text))
        })
        .try_fold(0i64, |sum, value| Some(sum + value?));
    let stated = cents(element.text_at(&["CtrlSum"]));
    if stated.is_none() || stated != summed {
        found.push(Violation::new(
            path,
            format!("{path}/CtrlSum is not the sum of the transfers it covers"),
        ));
    }
}

/// An identifier: present, within `Max35Text`, and spellable in the scheme's
/// character set.
fn identifier(path: &'static str, value: &str, found: &mut Vec<Violation>) {
    text(path, value, ID_MAX, found);
}

/// Text that is mandatory, bounded and restricted to the scheme's character
/// set. The character-set rule is the one that bites in practice: a name with
/// an umlaut in it is refused by the bank, not by us.
fn text(path: &'static str, value: &str, max: usize, found: &mut Vec<Violation>) {
    if value.trim().is_empty() {
        found.push(Violation::new(path, format!("{path} is empty")));
        return;
    }
    if value.chars().count() > max {
        found.push(Violation::new(
            path,
            format!("{path} is longer than the {max} characters the schema allows"),
        ));
    }
    if !value.chars().all(is_sepa_char) {
        found.push(Violation::new(
            path,
            format!("{path} carries a character outside the SEPA basic Latin set"),
        ));
    }
    if value.starts_with('/') || value.ends_with('/') || value.contains("//") {
        found.push(Violation::new(
            path,
            format!("{path} leads, trails or doubles a slash, which the scheme reserves"),
        ));
    }
}

/// A value the scheme fixes to exactly one code.
fn fixed(path: &'static str, value: &str, expected: &str, found: &mut Vec<Violation>) {
    if value != expected {
        found.push(Violation::new(
            path,
            format!("{path} must be {expected} in a SEPA credit transfer"),
        ));
    }
}

/// An account: identified by an IBAN, and by nothing else.
fn iban(path: &'static str, id: Option<&Element>, found: &mut Vec<Violation>) {
    let Some(id) = id else {
        found.push(Violation::new(path, format!("{path} states no account")));
        return;
    };
    let value = id.text_at(&["IBAN"]);
    if value.is_empty() {
        found.push(Violation::new(
            path,
            format!("{path} is not identified by an IBAN, which SEPA requires"),
        ));
        return;
    }
    let bytes = value.as_bytes();
    let shaped = (15..=34).contains(&value.len())
        && bytes[0].is_ascii_uppercase()
        && bytes[1].is_ascii_uppercase()
        && bytes[2].is_ascii_digit()
        && bytes[3].is_ascii_digit()
        && bytes.iter().all(u8::is_ascii_alphanumeric);
    if !shaped {
        found.push(Violation::new(
            path,
            format!("{path} does not carry an IBAN2007Identifier"),
        ));
    }
}

/// A bank: either a BIC of the right shape, or the scheme's own `NOTPROVIDED`
/// for an IBAN-only instruction. Never both, never neither.
fn agent(
    path: &'static str,
    element: Option<&Element>,
    version: Pain001Version,
    found: &mut Vec<Violation>,
) {
    let Some(element) = element else {
        found.push(Violation::new(path, format!("{path} is missing")));
        return;
    };
    let bic_tag = match version {
        Pain001Version::V03 => "BIC",
        Pain001Version::V09 => "BICFI",
    };
    let bic = element.text_at(&["FinInstnId", bic_tag]);
    let other = element.text_at(&["FinInstnId", "Othr", "Id"]);
    match (bic.is_empty(), other.is_empty()) {
        (true, true) => found.push(Violation::new(
            path,
            format!("{path} identifies no institution, not even as NOTPROVIDED"),
        )),
        (false, false) => found.push(Violation::new(
            path,
            format!("{path} states both a BIC and a substitute for one"),
        )),
        (false, true) => {
            let bytes = bic.as_bytes();
            let shaped = (bic.len() == 8 || bic.len() == 11)
                && bytes[..4].iter().all(u8::is_ascii_alphabetic)
                && bytes[4..6].iter().all(u8::is_ascii_alphabetic)
                && bytes[6..].iter().all(u8::is_ascii_alphanumeric);
            if !shaped {
                found.push(Violation::new(
                    path,
                    format!("{path} does not carry a BICFIIdentifier"),
                ));
            }
        }
        (true, false) => {
            if other != "NOTPROVIDED" {
                found.push(Violation::new(
                    path,
                    format!("{path} substitutes something other than NOTPROVIDED for a BIC"),
                ));
            }
        }
    }
}

/// Whether `c` is in the EPC basic Latin character set.
fn is_sepa_char(c: char) -> bool {
    matches!(c,
        'a'..='z' | 'A'..='Z' | '0'..='9'
        | '/' | '-' | '?' | ':' | '(' | ')' | '.' | ',' | '\'' | '+' | ' ')
}

/// A `YYYY-MM-DD` day, checked by shape and then by the calendar.
fn is_date(value: &str) -> bool {
    value.len() == 10
        && value.as_bytes()[4] == b'-'
        && value.as_bytes()[7] == b'-'
        && value
            .as_bytes()
            .iter()
            .enumerate()
            .all(|(at, byte)| matches!(at, 4 | 7) || byte.is_ascii_digit())
        && time::Date::parse(value, &time::format_description::well_known::Iso8601::DATE).is_ok()
}

/// A `YYYY-MM-DDThh:mm:ss` moment, with no fraction and no offset.
fn is_timestamp(value: &str) -> bool {
    let Some((day, clock)) = value.split_once('T') else {
        return false;
    };
    is_date(day)
        && clock.len() == 8
        && clock.as_bytes()[2] == b':'
        && clock.as_bytes()[5] == b':'
        && clock
            .as_bytes()
            .iter()
            .enumerate()
            .all(|(at, byte)| matches!(at, 2 | 5) || byte.is_ascii_digit())
}

/// A decimal amount as integer cents, or `None` when it is not one written the
/// way the schema writes one: digits, a point, exactly two digits.
fn cents(value: &str) -> Option<i64> {
    let (units, fraction) = value.split_once('.')?;
    if fraction.len() != 2 || units.is_empty() {
        return None;
    }
    if !units.bytes().all(|b| b.is_ascii_digit()) || !fraction.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let units: i64 = units.parse().ok()?;
    let fraction: i64 = fraction.parse().ok()?;
    units.checked_mul(100)?.checked_add(fraction)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A minimal, correct `.03` message — the fixture every rule below breaks
    /// exactly one thing in.
    fn message() -> String {
        r#"<?xml version="1.0" encoding="UTF-8"?>
<Document xmlns="urn:iso:std:iso:20022:tech:xsd:pain.001.001.03">
  <CstmrCdtTrfInitn>
    <GrpHdr>
      <MsgId>ALO20260807-ABCDEF012345</MsgId>
      <CreDtTm>2026-08-07T09:15:00</CreDtTm>
      <NbOfTxs>1</NbOfTxs>
      <CtrlSum>1331.97</CtrlSum>
      <InitgPty><Nm>Alo Werkplaats B.V.</Nm></InitgPty>
    </GrpHdr>
    <PmtInf>
      <PmtInfId>ALO20260807-ABCDEF012345-1</PmtInfId>
      <PmtMtd>TRF</PmtMtd>
      <BtchBookg>true</BtchBookg>
      <NbOfTxs>1</NbOfTxs>
      <CtrlSum>1331.97</CtrlSum>
      <PmtTpInf><SvcLvl><Cd>SEPA</Cd></SvcLvl></PmtTpInf>
      <ReqdExctnDt>2026-08-10</ReqdExctnDt>
      <Dbtr><Nm>Alo Werkplaats B.V.</Nm></Dbtr>
      <DbtrAcct><Id><IBAN>NL91ABNA0417164300</IBAN></Id></DbtrAcct>
      <DbtrAgt><FinInstnId><BIC>ABNANL2A</BIC></FinInstnId></DbtrAgt>
      <ChrgBr>SLEV</ChrgBr>
      <CdtTrfTxInf>
        <PmtId><EndToEndId>R-2026-77</EndToEndId></PmtId>
        <Amt><InstdAmt Ccy="EUR">1331.97</InstdAmt></Amt>
        <Cdtr><Nm>Muller + Sohne GmbH</Nm></Cdtr>
        <CdtrAcct><Id><IBAN>DE89370400440532013000</IBAN></Id></CdtrAcct>
        <RmtInf><Ustrd>R-2026-77</Ustrd></RmtInf>
      </CdtTrfTxInf>
    </PmtInf>
  </CstmrCdtTrfInitn>
</Document>"#
            .to_owned()
    }

    fn broken(xml: &str) -> Vec<String> {
        violations(xml, Pain001Version::V03)
            .into_iter()
            .map(|violation| format!("{}: {}", violation.rule, violation.detail))
            .collect()
    }

    #[test]
    fn a_correct_message_breaks_nothing() {
        assert_eq!(broken(&message()), Vec::<String>::new());
    }

    #[test]
    fn the_figures_have_to_describe_the_file_they_are_in() {
        let wrong_count = message().replace("<NbOfTxs>1</NbOfTxs>", "<NbOfTxs>2</NbOfTxs>");
        assert!(broken(&wrong_count).iter().any(|v| v.contains("NbOfTxs")));
        let wrong_sum =
            message().replacen("<CtrlSum>1331.97</CtrlSum>", "<CtrlSum>13.31</CtrlSum>", 1);
        assert!(broken(&wrong_sum).iter().any(|v| v.contains("CtrlSum")));
    }

    #[test]
    fn the_scheme_fixes_three_codes_and_one_currency() {
        for (from, to, expected) in [
            ("<PmtMtd>TRF</PmtMtd>", "<PmtMtd>CHK</PmtMtd>", "PmtMtd"),
            ("<ChrgBr>SLEV</ChrgBr>", "<ChrgBr>SHAR</ChrgBr>", "ChrgBr"),
            ("<Cd>SEPA</Cd>", "<Cd>URGP</Cd>", "SvcLvl"),
            ("Ccy=\"EUR\"", "Ccy=\"USD\"", "InstdAmt"),
        ] {
            let bent = message().replace(from, to);
            assert!(
                broken(&bent).iter().any(|v| v.contains(expected)),
                "{expected} was not caught"
            );
        }
    }

    #[test]
    fn a_name_a_bank_cannot_spell_is_caught_in_the_file_not_at_the_bank() {
        let umlaut = message().replace("Muller + Sohne GmbH", "Müller &amp; Söhne GmbH");
        let unspellable = broken(&umlaut);
        assert!(
            unspellable.iter().any(|v| v.contains("basic Latin")),
            "{unspellable:?}"
        );
        let long = message().replace("Muller + Sohne GmbH", &"A".repeat(71));
        assert!(broken(&long).iter().any(|v| v.contains("longer than")));
        let empty = message().replace("<Nm>Muller + Sohne GmbH</Nm>", "<Nm></Nm>");
        assert!(broken(&empty).iter().any(|v| v.contains("is empty")));
    }

    #[test]
    fn an_account_is_an_iban_and_a_bank_is_a_bic_or_says_it_is_not() {
        let not_an_iban = message().replace(
            "<IBAN>DE89370400440532013000</IBAN>",
            "<Othr><Id>0532013000</Othr></Id>",
        );
        assert!(!broken(&not_an_iban).is_empty());
        let bad_bic = message().replace("<BIC>ABNANL2A</BIC>", "<BIC>ABN1</BIC>");
        assert!(
            broken(&bad_bic)
                .iter()
                .any(|v| v.contains("BICFIIdentifier"))
        );
        // The IBAN-only instruction is correct, and so is nothing at all being
        // said about the creditor's bank.
        let unnamed = message().replace("<BIC>ABNANL2A</BIC>", "<Othr><Id>NOTPROVIDED</Id></Othr>");
        assert_eq!(broken(&unnamed), Vec::<String>::new());
        let invented =
            message().replace("<BIC>ABNANL2A</BIC>", "<Othr><Id>ASK-THE-BANK</Id></Othr>");
        assert!(broken(&invented).iter().any(|v| v.contains("NOTPROVIDED")));
    }

    #[test]
    fn a_sequence_is_a_sequence_and_not_a_set() {
        let swapped = message().replace(
            "<PmtMtd>TRF</PmtMtd>\n      <BtchBookg>true</BtchBookg>",
            "<BtchBookg>true</BtchBookg>\n      <PmtMtd>TRF</PmtMtd>",
        );
        assert!(
            broken(&swapped).iter().any(|v| v.contains("order")),
            "a misordered element is as fatal as a missing one"
        );
        let foreign = message().replace(
            "<ChrgBr>SLEV</ChrgBr>",
            "<ChrgBr>SLEV</ChrgBr>\n      <Purpose>salary</Purpose>",
        );
        assert!(broken(&foreign).iter().any(|v| v.contains("does not have")));
    }

    #[test]
    fn the_dates_are_the_standards_dates() {
        let stamped = message().replace(
            "<ReqdExctnDt>2026-08-10</ReqdExctnDt>",
            "<ReqdExctnDt>2026-08-10T00:00:00</ReqdExctnDt>",
        );
        assert!(broken(&stamped).iter().any(|v| v.contains("ISODate")));
        let zoned = message().replace("2026-08-07T09:15:00", "2026-08-07T09:15:00Z");
        assert!(broken(&zoned).iter().any(|v| v.contains("ISODateTime")));
        let impossible = message().replace(
            "<ReqdExctnDt>2026-08-10</ReqdExctnDt>",
            "<ReqdExctnDt>2026-02-31</ReqdExctnDt>",
        );
        assert!(broken(&impossible).iter().any(|v| v.contains("ISODate")));
    }

    #[test]
    fn an_amount_is_two_decimals_positive_and_below_the_ceiling() {
        for (bad, expected) in [
            ("1331.9", "two fraction digits"),
            ("1331", "two fraction digits"),
            ("-1331.97", "two fraction digits"),
            ("0.00", "positive"),
            ("1000000000.00", "ceiling"),
        ] {
            let bent = message().replace(">1331.97<", &format!(">{bad}<"));
            assert!(
                broken(&bent).iter().any(|v| v.contains(expected)),
                "{bad} was not caught"
            );
        }
        assert_eq!(cents("0.00"), Some(0));
        assert_eq!(cents("1331.97"), Some(133_197));
        assert_eq!(cents("1.5"), None);
        assert_eq!(cents(""), None);
    }

    #[test]
    fn the_wrong_version_is_the_wrong_file() {
        // The same bytes read as `.09`: the namespace is wrong, and so is the
        // shape of the execution date and the name of the BIC element.
        let misread = violations(&message(), Pain001Version::V09);
        assert!(misread.iter().any(|v| v.rule == "Document"));
        assert!(misread.iter().any(|v| v.detail.contains("ISODate")));
        assert!(misread.iter().any(|v| v.rule.contains("DbtrAgt")));
    }

    #[test]
    fn a_file_that_is_not_a_message_is_refused_rather_than_read() {
        assert!(!broken("not xml").is_empty());
        assert!(!broken("<Document></Document>").is_empty());
        assert!(!broken("<Nope><CstmrCdtTrfInitn/></Nope>").is_empty());
    }
}
