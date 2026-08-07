//! The SEPA credit-transfer instruction (alo Billing, wave B2.12) — an
//! ISO 20022 `pain.001` message written from a payment run.
//!
//! The third XML document this module writes, after the two e-invoice syntaxes
//! ([`crate::billing_cii`], [`crate::billing_ubl`]), and the only one that is
//! not a document *about* money but an instruction *to move* it. It is written
//! with the same emitter and the same discipline: the schema's sequence is the
//! caller's responsibility, nothing a supplier typed opens an element, and
//! every amount is formatted from integer cents.
//!
//! ## Two versions, deliberately
//!
//! `pain.001.001.03` is the version the EPC's customer-to-bank implementation
//! guidelines used from 2009 until the 2023 rulebook, and it is what nearly
//! every European bank's upload form still accepts today. `pain.001.001.09` is
//! the 2019 ISO version the 2023 guidelines moved to, and some banks now
//! require it. Which one a tenant needs is a fact about *their bank*, not about
//! their books, so it is a parameter rather than a decision we make for them —
//! and the difference between the two is genuinely small: the namespace, the
//! execution date gaining a `<Dt>` wrapper, and `BIC` being renamed `BICFI`.
//! The default is `.03`, because a file a bank cannot read is worth more
//! trouble than one it can.
//!
//! ## What is *not* in the message
//!
//! - **No `CdtrAgt` without a BIC.** Since 2016 a SEPA transfer is IBAN-only;
//!   the bank derives the creditor's institution. A bill states an account and
//!   almost never a BIC, and a BIC we guessed from the IBAN would be an
//!   invention in a payment instruction.
//! - **No structured creditor reference.** A supplier's ISO 11649 `RF…`
//!   reference is carried as it was stated, in the unstructured remittance
//!   line the whole scheme guarantees delivery of, rather than in
//!   `Strd/CdtrRefInf` — which needs the reference *validated* before it is
//!   claimed to be structured. That is its own item, recorded as a cut.
//! - **No batch-booking preference beyond one.** The run books as one debit,
//!   which is what a bookkeeper reconciling a bank statement wants.
//!
//! ## The character set
//!
//! A SEPA message may only carry `a–z A–Z 0–9 / - ? : ( ) . , ' +` and space
//! (EPC implementation guidelines, the "basic Latin character set"). A tenant's
//! data is not written in that set — `Söhne`, `Kraków`, `Meier & Co` — so every
//! text this module writes goes through [`sepa_text`], which folds accents to
//! their base letter, spells `ß` as `ss`, turns `&` into `+`, and replaces
//! anything left over with a space. That is a **presentation** rule and lives
//! here, never in the store: what the bank can spell and who was actually paid
//! are two different facts.

use time::OffsetDateTime;
use time::format_description::well_known::Iso8601;

use alo_store::billing_sepa::{PaymentFile, SEPA_ID_MAX_CHARS, SEPA_REMITTANCE_MAX_CHARS};

use crate::billing_xml::{Xml, amount};

/// Longest party name a SEPA message carries (EPC narrows ISO's 140).
pub const SEPA_NAME_MAX_CHARS: usize = 70;

/// The `pain.001` version a bank asks for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Pain001Version {
    /// `pain.001.001.03` — the long-standing EPC customer-to-bank version, and
    /// what a bank that states no preference accepts.
    #[default]
    V03,
    /// `pain.001.001.09` — the 2019 ISO version the 2023 EPC guidelines use.
    V09,
}

impl Pain001Version {
    /// The version a caller named, or `None` when it is not one we write.
    ///
    /// Both the full message identifier (`pain.001.001.03`) and the bare
    /// version (`03`, `9`) are accepted, because both are what a bank's own
    /// documentation calls it.
    #[must_use]
    pub fn parse(raw: &str) -> Option<Self> {
        match raw.trim() {
            "pain.001.001.03" | "03" | "3" => Some(Self::V03),
            "pain.001.001.09" | "09" | "9" => Some(Self::V09),
            _ => None,
        }
    }

    /// The message identifier, as the schema and every bank name it.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::V03 => "pain.001.001.03",
            Self::V09 => "pain.001.001.09",
        }
    }

    /// The namespace the document declares.
    #[must_use]
    pub fn namespace(self) -> String {
        format!("urn:iso:std:iso:20022:tech:xsd:{}", self.as_str())
    }

    /// What the debtor's bank is identified by: `BIC` in the old version,
    /// `BICFI` in the new one. The one element that was renamed under us.
    fn bic_tag(self) -> &'static str {
        match self {
            Self::V03 => "BIC",
            Self::V09 => "BICFI",
        }
    }
}

/// The name a payment file is saved under: the run's own identifier, which is
/// what the bank quotes back and what the bills carry.
#[must_use]
pub fn file_name(file: &PaymentFile) -> String {
    format!("sepa-credit-transfer-{}.xml", file.message_id)
}

/// Writes the payment run as a `pain.001` message.
///
/// `created_at` is the moment the file was produced (`GrpHdr/CreDtTm`), passed
/// in rather than read from the clock so the same run renders identically in a
/// test and in production.
#[must_use]
pub fn render(file: &PaymentFile, created_at: OffsetDateTime, version: Pain001Version) -> String {
    let mut xml = Xml::new();
    xml.raw("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n");
    xml.open_with(
        "Document",
        &format!(
            "xmlns=\"{}\" xmlns:xsi=\"http://www.w3.org/2001/XMLSchema-instance\"",
            version.namespace()
        ),
    );
    xml.open("CstmrCdtTrfInitn");
    group_header(&mut xml, file, created_at);
    payment_information(&mut xml, file, version);
    xml.close("CstmrCdtTrfInitn");
    xml.close("Document");
    xml.finish()
}

/// `GrpHdr` — who is instructing, when, and the two figures the bank checks the
/// rest of the file against.
fn group_header(xml: &mut Xml, file: &PaymentFile, created_at: OffsetDateTime) {
    xml.open("GrpHdr");
    xml.leaf("MsgId", &sepa_id(&file.message_id));
    xml.leaf("CreDtTm", &timestamp(created_at));
    xml.leaf("NbOfTxs", &file.count().to_string());
    xml.leaf("CtrlSum", &amount(file.control_sum_cents()));
    xml.open("InitgPty");
    xml.leaf("Nm", &sepa_name(&file.debtor_name));
    xml.close("InitgPty");
    xml.close("GrpHdr");
}

/// `PmtInf` — one debtor account, one execution date, and every transfer under
/// it. A payment run is that grouping by construction
/// ([`alo_store::billing_sepa`]), so the message has exactly one of these.
fn payment_information(xml: &mut Xml, file: &PaymentFile, version: Pain001Version) {
    xml.open("PmtInf");
    // Derived from the run's own identifier rather than minted separately: one
    // block, one identity, and a bank tracing a returned payment finds the same
    // string in both places.
    xml.leaf("PmtInfId", &sepa_id(&format!("{}-1", file.message_id)));
    xml.leaf("PmtMtd", "TRF");
    // One debit on the statement for the whole run, which is what makes it
    // reconcilable against the file that caused it.
    xml.leaf("BtchBookg", "true");
    xml.leaf("NbOfTxs", &file.count().to_string());
    xml.leaf("CtrlSum", &amount(file.control_sum_cents()));
    xml.open("PmtTpInf");
    xml.open("SvcLvl");
    xml.leaf("Cd", "SEPA");
    xml.close("SvcLvl");
    xml.close("PmtTpInf");
    let execution = date(file.execution_date);
    match version {
        Pain001Version::V03 => xml.leaf("ReqdExctnDt", &execution),
        Pain001Version::V09 => {
            xml.open("ReqdExctnDt");
            xml.leaf("Dt", &execution);
            xml.close("ReqdExctnDt");
        }
    }
    xml.open("Dbtr");
    xml.leaf("Nm", &sepa_name(&file.debtor_name));
    if !file.debtor_country.trim().is_empty() {
        xml.open("PstlAdr");
        xml.leaf("Ctry", file.debtor_country.trim());
        xml.close("PstlAdr");
    }
    xml.close("Dbtr");
    account(xml, "DbtrAcct", &file.debtor_iban);
    agent(xml, "DbtrAgt", &file.debtor_bic, version);
    // SLEV: the charges are shared as the scheme prescribes, which is the only
    // value a SEPA credit transfer may carry.
    xml.leaf("ChrgBr", "SLEV");
    for transfer in &file.transfers {
        xml.open("CdtTrfTxInf");
        xml.open("PmtId");
        xml.leaf("EndToEndId", &end_to_end(&transfer.end_to_end_id));
        xml.close("PmtId");
        xml.open("Amt");
        xml.leaf_with("InstdAmt", "Ccy=\"EUR\"", &amount(transfer.amount_cents));
        xml.close("Amt");
        if !transfer.creditor_bic.trim().is_empty() {
            agent(xml, "CdtrAgt", &transfer.creditor_bic, version);
        }
        xml.open("Cdtr");
        xml.leaf("Nm", &sepa_name(&transfer.creditor_name));
        xml.close("Cdtr");
        account(xml, "CdtrAcct", &transfer.creditor_iban);
        let remittance = sepa_text(&transfer.remittance, SEPA_REMITTANCE_MAX_CHARS);
        if !remittance.is_empty() {
            xml.open("RmtInf");
            xml.leaf("Ustrd", &remittance);
            xml.close("RmtInf");
        }
        xml.close("CdtTrfTxInf");
    }
    xml.close("PmtInf");
}

/// An account, identified by its IBAN and nothing else.
fn account(xml: &mut Xml, tag: &str, iban: &str) {
    xml.open(tag);
    xml.open("Id");
    xml.leaf("IBAN", iban);
    xml.close("Id");
    xml.close(tag);
}

/// A bank: its BIC when one is known, and the scheme's own "you work it out"
/// when it is not — which is what an IBAN-only instruction says.
fn agent(xml: &mut Xml, tag: &str, bic: &str, version: Pain001Version) {
    xml.open(tag);
    xml.open("FinInstnId");
    let bic = bic.trim();
    if bic.is_empty() {
        xml.open("Othr");
        xml.leaf("Id", "NOTPROVIDED");
        xml.close("Othr");
    } else {
        xml.leaf(version.bic_tag(), bic);
    }
    xml.close("FinInstnId");
    xml.close(tag);
}

// ---- the character set -------------------------------------------------------

/// Reduces text to what a SEPA message may carry, within `max` characters.
///
/// The scheme's basic Latin set is `a–z A–Z 0–9 / - ? : ( ) . , ' +` and space.
/// Everything else is folded rather than dropped, because a supplier called
/// `Müller & Söhne` must still be recognisable on the statement they read:
/// accents fold to their base letter, `ß` becomes `ss`, `&` becomes `+`, and
/// anything with no reading at all becomes a space. Runs of spaces collapse and
/// the result is trimmed, so a name that folds to nothing comes back empty and
/// the caller decides what to do about it.
///
/// `/` is additionally not allowed to lead, trail, or double, which is a rule
/// of the scheme rather than of the character set: a leading slash is how some
/// banks' parsers detect a code element.
#[must_use]
pub fn sepa_text(value: &str, max: usize) -> String {
    let mut out = String::with_capacity(value.len());
    for c in value.chars() {
        match c {
            'a'..='z' | 'A'..='Z' | '0'..='9' => out.push(c),
            '/' | '-' | '?' | ':' | '(' | ')' | '.' | ',' | '\'' | '+' | ' ' => out.push(c),
            // The substitutions a European name actually needs.
            '&' => out.push('+'),
            '_' | '–' | '—' => out.push('-'),
            '"' | '“' | '”' | '‘' | '’' | '«' | '»' => out.push('\''),
            '\n' | '\r' | '\t' => out.push(' '),
            c => match fold(c) {
                Some(base) => out.push_str(base),
                None => out.push(' '),
            },
        }
    }
    // Collapse the runs the folding may have produced, and refuse the slash
    // shapes the scheme reserves.
    let mut cleaned = String::with_capacity(out.len());
    for c in out.chars() {
        let previous = cleaned.chars().last();
        if (c == ' ' && previous == Some(' ')) || (c == '/' && previous == Some('/')) {
            continue;
        }
        cleaned.push(c);
    }
    let cleaned = cleaned.trim().trim_matches('/').trim();
    cleaned
        .chars()
        .take(max)
        .collect::<String>()
        .trim()
        .to_owned()
}

/// A party name, folded and cut to what the scheme carries.
fn sepa_name(value: &str) -> String {
    sepa_text(value, SEPA_NAME_MAX_CHARS)
}

/// An identifier we minted ourselves, folded defensively and cut to
/// `Max35Text`. Our own ids are already inside the set; this is the guard that
/// keeps that true if one ever stops being.
fn sepa_id(value: &str) -> String {
    sepa_text(value, SEPA_ID_MAX_CHARS)
}

/// The reference that travels to the supplier's ledger — the document number
/// they issued.
///
/// Never empty: `EndToEndId` is mandatory, and the scheme's own word for "there
/// is none" is `NOTPROVIDED`, which is better than a blank element a bank may
/// reject or a made-up value the supplier will not recognise.
fn end_to_end(value: &str) -> String {
    let folded = sepa_id(value);
    if folded.is_empty() {
        "NOTPROVIDED".to_owned()
    } else {
        folded
    }
}

/// Every letter that folds to one base letter, as `(the letters, the base)`.
///
/// A table rather than Unicode normalisation: the alternative is a
/// normalisation crate for a job that is one screen of data, and this way the
/// substitutions a bank will see are visible in the source and testable one by
/// one.
const FOLDED: &[(&str, &str)] = &[
    ("àáâãäåāăą", "a"),
    ("ÀÁÂÃÄÅĀĂĄ", "A"),
    ("çćĉċč", "c"),
    ("ÇĆĈĊČ", "C"),
    ("ďđ", "d"),
    ("ĎĐ", "D"),
    ("èéêëēĕėęě", "e"),
    ("ÈÉÊËĒĔĖĘĚ", "E"),
    ("ĝğġģ", "g"),
    ("ĜĞĠĢ", "G"),
    ("ĥħ", "h"),
    ("ĤĦ", "H"),
    ("ìíîïĩīĭįı", "i"),
    ("ÌÍÎÏĨĪĬĮİ", "I"),
    ("ĵ", "j"),
    ("Ĵ", "J"),
    ("ķ", "k"),
    ("Ķ", "K"),
    ("ĺļľŀł", "l"),
    ("ĹĻĽĿŁ", "L"),
    ("ñńņňŉ", "n"),
    ("ÑŃŅŇ", "N"),
    ("òóôõöøōŏőð", "o"),
    ("ÒÓÔÕÖØŌŎŐÐ", "O"),
    ("ŕŗř", "r"),
    ("ŔŖŘ", "R"),
    ("śŝşš", "s"),
    ("ŚŜŞŠ", "S"),
    ("ţťŧ", "t"),
    ("ŢŤŦ", "T"),
    ("ùúûüũūŭůűų", "u"),
    ("ÙÚÛÜŨŪŬŮŰŲ", "U"),
    ("ŵ", "w"),
    ("Ŵ", "W"),
    ("ýÿŷ", "y"),
    ("ÝŸŶ", "Y"),
    ("źżž", "z"),
    ("ŹŻŽ", "Z"),
    // The ligatures and the letters that are two letters when spelt out.
    ("ß", "ss"),
    ("æ", "ae"),
    ("Æ", "AE"),
    ("œ", "oe"),
    ("Œ", "OE"),
    ("þ", "th"),
    ("Þ", "TH"),
    ("ĳ", "ij"),
    ("Ĳ", "IJ"),
];

/// What `c` is spelt as in the scheme's character set, or `None` when it has no
/// reading there at all.
fn fold(c: char) -> Option<&'static str> {
    FOLDED
        .iter()
        .find(|(letters, _)| letters.contains(c))
        .map(|(_, base)| *base)
}

// ---- the standard's own formats ----------------------------------------------

/// A day, as `YYYY-MM-DD` (`ISODate`).
fn date(value: time::Date) -> String {
    value.format(&Iso8601::DATE).unwrap_or_default()
}

/// A moment, as `YYYY-MM-DDThh:mm:ss` in UTC (`ISODateTime`).
///
/// No fractional seconds and no offset: both are permitted by the schema and
/// both are refused by some banks' parsers, and nothing in a payment file needs
/// either.
fn timestamp(value: OffsetDateTime) -> String {
    let utc = value.to_offset(time::UtcOffset::UTC);
    format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}",
        utc.year(),
        u8::from(utc.month()),
        utc.day(),
        utc.hour(),
        utc.minute(),
        utc.second()
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use alo_store::BillingBillId;
    use alo_store::billing_sepa::CreditTransfer;
    use time::{Date, Month};

    fn day(year: i32, month: u8, day: u8) -> Date {
        Date::from_calendar_date(year, Month::try_from(month).unwrap_or(Month::January), day)
            .unwrap_or(Date::MIN)
    }

    fn file() -> PaymentFile {
        PaymentFile {
            message_id: "ALO20260807-ABCDEF012345".to_owned(),
            execution_date: day(2026, 8, 10),
            debtor_name: "Alo Werkplaats B.V.".to_owned(),
            debtor_iban: "NL91ABNA0417164300".to_owned(),
            debtor_bic: "ABNANL2A".to_owned(),
            debtor_country: "NL".to_owned(),
            transfers: vec![CreditTransfer {
                bill_id: BillingBillId::new("b-1".to_owned()),
                creditor_name: "Müller & Söhne GmbH".to_owned(),
                creditor_iban: "DE89370400440532013000".to_owned(),
                creditor_bic: String::new(),
                amount_cents: 133_197,
                end_to_end_id: "R-2026-77".to_owned(),
                remittance: "R-2026-77".to_owned(),
            }],
        }
    }

    fn rendered(version: Pain001Version) -> String {
        render(&file(), OffsetDateTime::UNIX_EPOCH, version)
    }

    #[test]
    fn the_two_versions_differ_in_exactly_three_places() {
        let old = rendered(Pain001Version::V03);
        let new = rendered(Pain001Version::V09);
        assert!(old.contains("urn:iso:std:iso:20022:tech:xsd:pain.001.001.03"));
        assert!(new.contains("urn:iso:std:iso:20022:tech:xsd:pain.001.001.09"));
        assert!(old.contains("<ReqdExctnDt>2026-08-10</ReqdExctnDt>"));
        assert!(new.contains("<ReqdExctnDt>\n"), "the date gains a wrapper");
        assert!(new.contains("<Dt>2026-08-10</Dt>"));
        assert!(old.contains("<BIC>ABNANL2A</BIC>"));
        assert!(new.contains("<BICFI>ABNANL2A</BICFI>"));
        // …and in nothing else: the payment itself is the same instruction.
        for xml in [&old, &new] {
            assert!(xml.contains("<InstdAmt Ccy=\"EUR\">1331.97</InstdAmt>"));
            assert!(xml.contains("<IBAN>DE89370400440532013000</IBAN>"));
            assert!(xml.contains("<ChrgBr>SLEV</ChrgBr>"));
            assert!(xml.contains("<Cd>SEPA</Cd>"));
            assert!(xml.contains("<PmtMtd>TRF</PmtMtd>"));
        }
    }

    #[test]
    fn a_version_is_named_the_way_a_bank_names_it() {
        assert_eq!(
            Pain001Version::parse("pain.001.001.03"),
            Some(Pain001Version::V03)
        );
        assert_eq!(Pain001Version::parse(" 09 "), Some(Pain001Version::V09));
        assert_eq!(Pain001Version::parse("3"), Some(Pain001Version::V03));
        assert_eq!(Pain001Version::parse("pain.001.001.11"), None);
        assert_eq!(Pain001Version::parse(""), None);
        assert_eq!(Pain001Version::default(), Pain001Version::V03);
    }

    #[test]
    fn a_bank_with_no_bic_is_told_so_in_the_schemes_own_word() {
        let mut file = file();
        file.debtor_bic = String::new();
        let xml = render(&file, OffsetDateTime::UNIX_EPOCH, Pain001Version::V03);
        assert!(xml.contains("<Id>NOTPROVIDED</Id>"));
        assert!(!xml.contains("<BIC>"), "no invented institution");
        // The creditor's bank is never stated at all when the bill did not.
        assert!(!xml.contains("<CdtrAgt>"));
    }

    #[test]
    fn a_name_the_scheme_cannot_spell_is_folded_not_dropped() {
        assert_eq!(sepa_text("Müller & Söhne GmbH", 70), "Muller + Sohne GmbH");
        assert_eq!(sepa_text("Straße 5", 70), "Strasse 5");
        assert_eq!(sepa_text("Kraków Sp. z o.o.", 70), "Krakow Sp. z o.o.");
        assert_eq!(sepa_text("Ærø Håndværk A/S", 70), "AEro Handvaerk A/S");
        assert_eq!(sepa_text("Société Générale", 70), "Societe Generale");
        // Anything with no reading becomes one space, and runs collapse.
        assert_eq!(sepa_text("ΑΒΓ Ltd", 70), "Ltd");
        assert_eq!(sepa_text("a\u{0}\u{0}b", 70), "a b");
        // The slash rules of the scheme.
        assert_eq!(sepa_text("/leading", 70), "leading");
        assert_eq!(sepa_text("a//b", 70), "a/b");
        assert_eq!(sepa_text("trailing/", 70), "trailing");
        // Truncation never leaves a trailing space behind.
        assert_eq!(sepa_text("abcdef ghij", 7), "abcdef");
        // A name with nothing left is empty rather than a guess.
        assert_eq!(sepa_text("№№№", 70), "");
    }

    #[test]
    fn an_end_to_end_reference_is_always_stated() {
        assert_eq!(end_to_end("R-2026/77"), "R-2026/77");
        assert_eq!(end_to_end("№"), "NOTPROVIDED");
        assert_eq!(end_to_end(""), "NOTPROVIDED");
        assert_eq!(end_to_end(&"A".repeat(40)).chars().count(), 35);
    }

    #[test]
    fn the_figures_the_bank_checks_the_file_against_are_the_files_own() {
        let mut file = file();
        file.transfers.push(CreditTransfer {
            bill_id: BillingBillId::new("b-2".to_owned()),
            amount_cents: 2_500,
            end_to_end_id: "R-2026-78".to_owned(),
            remittance: String::new(),
            ..file.transfers[0].clone()
        });
        let xml = render(&file, OffsetDateTime::UNIX_EPOCH, Pain001Version::V03);
        assert_eq!(xml.matches("<NbOfTxs>2</NbOfTxs>").count(), 2);
        assert_eq!(xml.matches("<CtrlSum>1356.97</CtrlSum>").count(), 2);
        // A transfer with nothing to say says nothing, rather than an empty
        // element a bank may refuse.
        assert_eq!(xml.matches("<RmtInf>").count(), 1);
    }

    #[test]
    fn a_moment_is_written_without_a_zone_and_a_day_without_a_time() {
        assert_eq!(timestamp(OffsetDateTime::UNIX_EPOCH), "1970-01-01T00:00:00");
        assert_eq!(date(day(2026, 8, 10)), "2026-08-10");
    }

    #[test]
    fn the_file_is_named_after_the_run_the_bank_will_quote() {
        assert_eq!(
            file_name(&file()),
            "sepa-credit-transfer-ALO20260807-ABCDEF012345.xml"
        );
    }
}
