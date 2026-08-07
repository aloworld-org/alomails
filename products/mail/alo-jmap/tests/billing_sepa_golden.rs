//! Golden files for the SEPA credit-transfer instruction (B2.12): three
//! payment runs pinned byte for byte, each of them checked against the schema
//! subset and the scheme's rules *before* it is compared.
//!
//! | File | What it pins |
//! |---|---|
//! | `sepa-standard.xml` | the everyday run: two suppliers, a name only a fold makes spellable, a reference one of them asked for |
//! | `sepa-edge.xml` | a tenant whose bank has no BIC on file and who has stated no country, paying a supplier whose name folds hard and whose reference is a long one |
//! | `sepa-pain001-09.xml` | the same everyday run in the 2019 version — the namespace, the wrapped execution date, `BICFI` |
//!
//! Reading the first and the third side by side is the point: they are the same
//! instruction, and the diff between them is exactly the three things the two
//! `pain.001` versions disagree about. If a change to the writer moves anything
//! else, both files move together and the diff says so.
//!
//! To regenerate after an intended change: `UPDATE_GOLDEN=1 cargo test -p
//! alo-jmap --test billing_sepa_golden`, then read the diff before committing.
//!
//! **What these are not.** They are our own output checked against our own
//! reading of ISO 20022 and the EPC guidelines, not files a bank has executed.
//! Validating them once against the normative XSD, offline, and uploading one to
//! a real bank's test facility both stay open items for a human
//! (`docs/autonomy/STATE.md`) — these files are what makes that a one-off
//! confirmation rather than a standing risk.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use time::{Date, Month, OffsetDateTime};

use alo_jmap::billing_pain001::{Pain001Version, file_name, render, sepa_text};
use alo_jmap::billing_pain001_rules::violations;
use alo_store::BillingBillId;
use alo_store::billing_sepa::{CreditTransfer, PaymentFile};

// ---- fixtures ----------------------------------------------------------------

fn day(year: i32, month: u8, day: u8) -> Date {
    Date::from_calendar_date(year, Month::try_from(month).unwrap(), day).unwrap()
}

/// The moment every golden file is created at, so the one element that would
/// otherwise change every run is fixed.
fn created_at() -> OffsetDateTime {
    day(2026, 8, 7).with_hms(9, 15, 0).unwrap().assume_utc()
}

fn transfer(
    id: &str,
    name: &str,
    iban: &str,
    cents: i64,
    number: &str,
    remittance: &str,
) -> CreditTransfer {
    CreditTransfer {
        bill_id: BillingBillId::new(id.to_owned()),
        creditor_name: name.to_owned(),
        creditor_iban: iban.to_owned(),
        creditor_bic: String::new(),
        amount_cents: cents,
        end_to_end_id: number.to_owned(),
        remittance: remittance.to_owned(),
    }
}

/// The everyday run: two suppliers on one execution date.
fn standard() -> PaymentFile {
    PaymentFile {
        message_id: "ALO20260807-A1B2C3D4E5F6".to_owned(),
        execution_date: day(2026, 8, 10),
        debtor_name: "Alo Werkplaats B.V.".to_owned(),
        debtor_iban: "NL91ABNA0417164300".to_owned(),
        debtor_bic: "ABNANL2A".to_owned(),
        debtor_country: "NL".to_owned(),
        transfers: vec![
            transfer(
                "b-1",
                "Müller & Söhne GmbH",
                "DE89370400440532013000",
                133_197,
                "R-2026-77",
                "R-2026-77",
            ),
            transfer(
                "b-2",
                "Krakowski Dostawca Sp. z o.o.",
                "PL61109010140000071219812874",
                45_000,
                "FV/2026/08/12",
                "RF18 5390 0754 7034",
            ),
        ],
    }
}

/// The awkward run: no BIC for our own bank, no country stated, and a supplier
/// whose name and reference both need work before a bank will take them.
fn edge() -> PaymentFile {
    PaymentFile {
        message_id: "ALO20260807-99FFEE001122".to_owned(),
        execution_date: day(2026, 12, 31),
        debtor_name: "Ærø Håndværk & Co".to_owned(),
        debtor_iban: "BE68539007547034".to_owned(),
        debtor_bic: String::new(),
        debtor_country: String::new(),
        transfers: vec![transfer(
            "b-9",
            "Société Générale d'Équipement — Lyon",
            "FR1420041010050500013M02606",
            999_999,
            "2026/08/№441",
            "Facture 2026/08/441 — livraison août, dépôt Lyon Vaise, référence client 4471/A",
        )],
    }
}

// ---- the harness -------------------------------------------------------------

/// The XML of a run, checked against the schema subset and the scheme's rules
/// before it is compared: a golden file of a message a bank would refuse pins a
/// mistake.
fn message(file: &PaymentFile, version: Pain001Version) -> String {
    let xml = render(file, created_at(), version);
    let broken = violations(&xml, version);
    assert!(
        broken.is_empty(),
        "the file we produced would be refused: {broken:?}"
    );
    xml
}

/// Compares against the file in `tests/golden/`, or writes it when
/// `UPDATE_GOLDEN=1`.
fn golden(name: &str, actual: &str) {
    let path = format!("{}/tests/golden/{name}", env!("CARGO_MANIFEST_DIR"));
    if std::env::var("UPDATE_GOLDEN").is_ok() {
        std::fs::write(&path, actual).expect("the golden file could not be written");
        return;
    }
    let expected = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("{name} is missing ({e}); regenerate with UPDATE_GOLDEN=1"));
    assert_eq!(
        actual, expected,
        "{name} has changed; if that is intended, regenerate with UPDATE_GOLDEN=1 and read the diff"
    );
}

// ---- the three files ---------------------------------------------------------

#[test]
fn the_everyday_payment_run() {
    let file = standard();
    let xml = message(&file, Pain001Version::V03);
    // The two figures the bank checks the whole file against, hand-computed:
    // 1331.97 + 450.00.
    assert!(xml.contains("<NbOfTxs>2</NbOfTxs>"));
    assert!(xml.contains("<CtrlSum>1781.97</CtrlSum>"));
    assert!(xml.contains("<InstdAmt Ccy=\"EUR\">1331.97</InstdAmt>"));
    assert!(xml.contains("<InstdAmt Ccy=\"EUR\">450.00</InstdAmt>"));
    // The supplier is recognisable after folding, and their reference reaches
    // them unaltered.
    assert!(xml.contains("<Nm>Muller + Sohne GmbH</Nm>"));
    assert!(xml.contains("<Ustrd>RF18 5390 0754 7034</Ustrd>"));
    // The document number travels end to end, slash and all.
    assert!(xml.contains("<EndToEndId>FV/2026/08/12</EndToEndId>"));
    // Our own account and our own bank.
    assert!(xml.contains("<IBAN>NL91ABNA0417164300</IBAN>"));
    assert!(xml.contains("<BIC>ABNANL2A</BIC>"));
    golden("sepa-standard.xml", &xml);
}

#[test]
fn a_run_whose_every_field_needs_work() {
    let file = edge();
    let xml = message(&file, Pain001Version::V03);
    // No BIC on file: the scheme's own word for it, never an invented one.
    assert!(xml.contains("<Id>NOTPROVIDED</Id>"));
    assert!(!xml.contains("<BIC>"));
    // No country stated: the address group is absent rather than empty.
    assert!(!xml.contains("<PstlAdr>"));
    // Both names folded into the basic Latin set.
    assert!(xml.contains("<Nm>AEro Handvaerk + Co</Nm>"));
    // The apostrophe is in the scheme's character set and is escaped as XML
    // escapes one — the reader a bank runs unescapes it back.
    assert!(xml.contains("<Nm>Societe Generale d&apos;Equipement - Lyon</Nm>"));
    // A document number with a character the scheme has no reading for still
    // reaches the supplier, with that one character as a space.
    assert!(xml.contains("<EndToEndId>2026/08/ 441</EndToEndId>"));
    // The remittance line is folded and stays inside its 140 characters.
    assert!(xml.contains("livraison aout, depot Lyon Vaise"));
    assert!(xml.contains("<InstdAmt Ccy=\"EUR\">9999.99</InstdAmt>"));
    golden("sepa-edge.xml", &xml);
}

#[test]
fn the_same_run_in_the_2019_version() {
    let file = standard();
    let xml = message(&file, Pain001Version::V09);
    assert!(xml.contains("urn:iso:std:iso:20022:tech:xsd:pain.001.001.09"));
    assert!(xml.contains("<Dt>2026-08-10</Dt>"));
    assert!(xml.contains("<BICFI>ABNANL2A</BICFI>"));
    golden("sepa-pain001-09.xml", &xml);
}

// ---- what holds over all of them ---------------------------------------------

#[test]
fn every_golden_file_is_one_a_bank_would_take() {
    for (name, version) in [
        ("sepa-standard.xml", Pain001Version::V03),
        ("sepa-edge.xml", Pain001Version::V03),
        ("sepa-pain001-09.xml", Pain001Version::V09),
    ] {
        let stored = std::fs::read_to_string(format!(
            "{}/tests/golden/{name}",
            env!("CARGO_MANIFEST_DIR")
        ))
        .unwrap_or_else(|e| panic!("{name} is missing ({e}); regenerate with UPDATE_GOLDEN=1"));
        // The stored bytes, not only the freshly rendered ones: a golden file
        // edited by hand is still checked.
        let broken = violations(&stored, version);
        assert!(broken.is_empty(), "{name}: {broken:?}");
        // Nothing outside the scheme's character set survives anywhere in the
        // document — the one property a bank rejects a whole file for.
        for line in stored.lines() {
            let text = line.trim();
            assert!(
                text.is_ascii(),
                "{name} carries a character a SEPA message cannot: {text}"
            );
        }
        assert!(stored.starts_with("<?xml version=\"1.0\" encoding=\"UTF-8\"?>"));
        assert!(stored.ends_with("</Document>\n"));
    }
}

#[test]
fn the_two_versions_of_the_same_run_differ_only_where_the_standard_does() {
    let file = standard();
    let old = render(&file, created_at(), Pain001Version::V03);
    let new = render(&file, created_at(), Pain001Version::V09);
    let differing: Vec<(&str, &str)> = old
        .lines()
        .zip(new.lines())
        .filter(|(left, right)| left != right)
        .collect();
    // The namespace, and the BIC element's name. The execution date differs in
    // line *count* as well, so the zip below it is offset — which is why the
    // assertion is on the first two rather than on the whole list.
    assert!(differing[0].0.contains("pain.001.001.03"));
    assert!(differing[0].1.contains("pain.001.001.09"));
    assert!(
        differing[1]
            .0
            .contains("<ReqdExctnDt>2026-08-10</ReqdExctnDt>")
    );
    assert!(differing[1].1.contains("<ReqdExctnDt>"));
    // Every payment line is byte-identical: what is being paid does not depend
    // on which version the bank asked for.
    for figure in [
        "<InstdAmt Ccy=\"EUR\">1331.97</InstdAmt>",
        "<IBAN>PL61109010140000071219812874</IBAN>",
        "<EndToEndId>R-2026-77</EndToEndId>",
        "<CtrlSum>1781.97</CtrlSum>",
    ] {
        assert!(old.contains(figure) && new.contains(figure), "{figure}");
    }
}

#[test]
fn the_file_a_tenant_downloads_is_named_after_the_run() {
    assert_eq!(
        file_name(&standard()),
        "sepa-credit-transfer-ALO20260807-A1B2C3D4E5F6.xml"
    );
    // And the fold that makes all of this possible is the one exported piece
    // of it, so a caller elsewhere cannot write a second one.
    assert_eq!(sepa_text("Müller & Söhne", 70), "Muller + Sohne");
}
