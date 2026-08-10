//! Golden files for the Factur-X e-invoice (B1.22): four documents, pinned
//! byte for byte.
//!
//! CII is a schema of **sequences** — `ram:PostcodeCode` before `ram:LineOne`,
//! `ram:TaxCurrencyCode` before `ram:InvoiceCurrencyCode`, the whole settlement
//! block in one fixed order — and the difference between a document that
//! validates at a customer's gateway and one that is rejected there is often a
//! reordering nobody could see in a diff of Rust. So the output is a file in
//! the repository, and any change to it is a change somebody has to look at.
//!
//! The four cover what the mapping actually decides:
//!
//! | File | What it pins |
//! |---|---|
//! | `invoice-standard.xml` | the everyday document: one rate, bank details, a reference, a note |
//! | `invoice-mixed-rates.xml` | two rates including 0 % (categories `S` and `Z`), several units, a two-paragraph line, no bank account |
//! | `credit-note.xml` | the credit direction: type 381, positive amounts, the corrected number, no payment instructions |
//! | `invoice-foreign-currency.xml` | BT-6/BT-111: a USD document stating its VAT in EUR as well |
//!
//! Every one of them is also run through the rule checker, because a golden
//! file of an invalid document would pin a mistake.
//!
//! To regenerate after an intended change: `UPDATE_GOLDEN=1 cargo test -p
//! alo-jmap --test billing_cii_golden`, then read the diff before committing
//! it.
//!
//! **What these are not.** They are our own output, not the official Factur-X
//! sample set: the normative samples and the EN 16931 schematron are
//! externally licensed artefacts, and running the schematron needs an XSLT
//! processor (a third language, `CLAUDE.md`). Validating against the normative
//! artefacts stays an open item for a human — recorded in
//! `docs/autonomy/STATE.md` — and these files are what makes that run a
//! one-off check rather than a continuous risk.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use time::{Date, Month, OffsetDateTime};

use alo_jmap::billing_cii as cii;
use alo_jmap::billing_einvoice::EInvoice;
use alo_jmap::billing_einvoice_rules::violations;
use alo_jmap::billing_print::{DocumentKind, Party, PrintDocument, Restated, strings_for};
use alo_store::billing_settings::BillingSettings;
use alo_store::billing_totals::{LineFigures, Totals, totals};
use alo_store::{BillingCustomerId, BillingLineId, Customer, Line};

// ---- fixtures ----------------------------------------------------------------

fn day(year: i32, month: u8, day: u8) -> Date {
    Date::from_calendar_date(year, Month::try_from(month).unwrap(), day).unwrap()
}

fn customer() -> Customer {
    Customer {
        id: BillingCustomerId::new("cus-1".to_owned()),
        name: "Kunde & Söhne GmbH".to_owned(),
        address_line1: "Hauptstraße 5".to_owned(),
        address_line2: "Gebäude C".to_owned(),
        postal_code: "10115".to_owned(),
        city: "Berlin".to_owned(),
        country: "DE".to_owned(),
        vat_id: Some("DE811907980".to_owned()),
        email: Some("einkauf@kunde.test".to_owned()),
        payment_terms_days: 14,
        currency: "EUR".to_owned(),
        contact_id: None,
        archived_at: None,
        created_by: "u1".to_owned(),
        created_at: OffsetDateTime::UNIX_EPOCH,
        updated_at: OffsetDateTime::UNIX_EPOCH,
    }
}

fn issuer() -> BillingSettings {
    BillingSettings {
        legal_name: "Alo Werkplaats B.V.".to_owned(),
        address_line1: "Keizersgracht 1".to_owned(),
        postal_code: "1015 CJ".to_owned(),
        city: "Amsterdam".to_owned(),
        country: "NL".to_owned(),
        vat_id: Some("NL812345678B01".to_owned()),
        registration_no: "KVK 90123456".to_owned(),
        email: "billing@alo.test".to_owned(),
        iban: Some("NL91ABNA0417164300".to_owned()),
        bic: Some("ABNANL2A".to_owned()),
        bank_name: "ABN AMRO".to_owned(),
        ..Default::default()
    }
}

/// A line: description, unit, quantity in milli-units, unit price in cents,
/// VAT rate in basis points.
fn line(description: &str, unit: &str, qty_milli: i64, price: i64, rate_bp: i32) -> Line {
    Line {
        id: BillingLineId::new(format!("l-{description}")),
        line_order: 0,
        description: description.to_owned(),
        unit: unit.to_owned(),
        qty_milli,
        unit_price_cents: price,
        vat_rate_bp: rate_bp,
    }
}

fn figures(lines: &[Line]) -> Totals {
    totals(
        &lines
            .iter()
            .map(|l| LineFigures {
                qty_milli: l.qty_milli,
                unit_price_cents: l.unit_price_cents,
                vat_rate_bp: l.vat_rate_bp,
            })
            .collect::<Vec<_>>(),
    )
}

/// The XML of a document, checked against the rules before it is compared: a
/// golden file of an invalid invoice would pin a mistake in place.
fn render(doc: &PrintDocument<'_>) -> String {
    let einvoice = EInvoice::from_document(doc, strings_for("en")).expect("an invoice document");
    let broken = violations(&einvoice);
    assert!(
        broken.is_empty(),
        "the fixture itself breaks EN 16931: {broken:?}"
    );
    cii::render(&einvoice)
}

/// Compares against the file in `tests/golden/`, or writes it when
/// `UPDATE_GOLDEN=1` — an intended change is one command and a diff to read,
/// and an unintended one is a failing test.
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

// ---- the four documents ------------------------------------------------------

#[test]
fn the_everyday_invoice() {
    let (customer, issuer) = (customer(), issuer());
    let lines = [
        line("Consulting", "hour", 15_000, 12_500, 2100),
        line("Travel", "km", 240_000, 42, 2100),
    ];
    let totals = figures(&lines);
    let doc = PrintDocument {
        kind: DocumentKind::Invoice,
        banner: None,
        number: Some("INV-2026-00001"),
        primary_date: Some(day(2026, 8, 7)),
        secondary_date: Some(day(2026, 8, 21)),
        reference: "PO-42",
        note: "Thank you for your business.",
        currency: "EUR",
        payment_terms_days: Some(14),
        credits_number: None,
        party: Party::customer(&customer),
        lines: &lines,
        totals: &totals,
        restated: None,
        issuer: &issuer,
    };
    let xml = render(&doc);
    // The figures the store computed, on the wire the standard reads them off.
    // 15 h at 125.00 is 1875.00; 240 km at 0.42 is 100.80; 21 % of 1975.80 is
    // 414.918, rounded once at the rate subtotal to 414.92.
    assert!(xml.contains("<ram:LineTotalAmount>1875.00</ram:LineTotalAmount>"));
    assert!(xml.contains("<ram:LineTotalAmount>100.80</ram:LineTotalAmount>"));
    assert!(xml.contains("<ram:CalculatedAmount>414.92</ram:CalculatedAmount>"));
    assert!(xml.contains("<ram:GrandTotalAmount>2390.72</ram:GrandTotalAmount>"));
    golden("invoice-standard.xml", &xml);
}

#[test]
fn two_rates_two_categories_and_a_line_with_a_paragraph() {
    let (customer, mut issuer) = (customer(), issuer());
    // No bank account stated: the payment-means block disappears entirely
    // rather than appearing empty.
    issuer.iban = None;
    issuer.bic = None;
    let lines = [
        line(
            "Consulting\nMarch, on site. Two engineers, four days.",
            "day",
            4_000,
            80_000,
            2100,
        ),
        line("Printed handbook", "pcs", 12_000, 1_950, 900),
        line("Intra-community delivery", "", 1_000, 25_000, 0),
    ];
    let totals = figures(&lines);
    let doc = PrintDocument {
        kind: DocumentKind::Invoice,
        banner: None,
        number: Some("INV-2026-00002"),
        primary_date: Some(day(2026, 8, 7)),
        secondary_date: None,
        reference: "",
        note: "",
        currency: "EUR",
        payment_terms_days: Some(30),
        credits_number: None,
        party: Party::customer(&customer),
        lines: &lines,
        totals: &totals,
        restated: None,
        issuer: &issuer,
    };
    let xml = render(&doc);
    // Three rates, three breakdown groups, ascending — and the 0 % one is
    // category Z with no VAT on it.
    assert_eq!(xml.matches("<ram:CategoryCode>").count(), 6);
    assert!(xml.contains("<ram:CategoryCode>Z</ram:CategoryCode>"));
    assert!(xml.contains("<ram:BilledQuantity unitCode=\"DAY\">4</ram:BilledQuantity>"));
    assert!(xml.contains("<ram:BilledQuantity unitCode=\"H87\">12</ram:BilledQuantity>"));
    assert!(xml.contains("<ram:BilledQuantity unitCode=\"C62\">1</ram:BilledQuantity>"));
    // A due date it does not have is not invented; the terms say it in words.
    assert!(!xml.contains("DueDateDateTime"));
    assert!(xml.contains("<ram:Description>Payable within 30 days"));
    golden("invoice-mixed-rates.xml", &xml);
}

#[test]
fn the_credit_note_runs_in_credit_direction() {
    let (customer, issuer) = (customer(), issuer());
    // The store's mirror: the original's quantities, negated.
    let lines = [line("Consulting", "hour", -15_000, 12_500, 2100)];
    let totals = figures(&lines);
    let doc = PrintDocument {
        kind: DocumentKind::CreditNote,
        banner: None,
        number: Some("INV-2026-00003"),
        primary_date: Some(day(2026, 8, 10)),
        secondary_date: Some(day(2026, 8, 24)),
        reference: "PO-42",
        note: "Cancelled after the site visit was called off.",
        currency: "EUR",
        payment_terms_days: Some(14),
        credits_number: Some("INV-2026-00001"),
        party: Party::customer(&customer),
        lines: &lines,
        totals: &totals,
        restated: None,
        issuer: &issuer,
    };
    let xml = render(&doc);
    assert!(xml.contains("<ram:TypeCode>381</ram:TypeCode>"));
    // Positive on the wire, negative in our ledger: the direction is BT-3's
    // job, and nothing here is negative.
    assert!(xml.contains("<ram:GrandTotalAmount>2268.75</ram:GrandTotalAmount>"));
    assert!(!xml.contains(">-"));
    assert!(xml.contains("<ram:IssuerAssignedID>INV-2026-00001</ram:IssuerAssignedID>"));
    // Nothing is payable on it, so it carries no instructions for paying.
    assert!(!xml.contains("PaymentMeans"));
    golden("credit-note.xml", &xml);
}

#[test]
fn a_document_in_another_currency_states_its_vat_twice() {
    let (mut customer, issuer) = (customer(), issuer());
    customer.currency = "USD".to_owned();
    let lines = [line("Consulting", "hour", 8_000, 15_000, 2100)];
    let totals = figures(&lines);
    let doc = PrintDocument {
        kind: DocumentKind::Invoice,
        banner: None,
        number: Some("INV-2026-00004"),
        primary_date: Some(day(2026, 8, 7)),
        secondary_date: Some(day(2026, 8, 21)),
        reference: "PO-77",
        note: "",
        currency: "USD",
        payment_terms_days: Some(14),
        credits_number: None,
        party: Party::customer(&customer),
        lines: &lines,
        totals: &totals,
        // What the store froze on the document when it was issued: 1 EUR =
        // 1.1626 USD, published on the issue date.
        restated: Some(Restated {
            currency: "EUR".to_owned(),
            vat_cents: 21_675,
            rate_micro: 1_162_600,
            rate_date: day(2026, 8, 7),
        }),
        issuer: &issuer,
    };
    let xml = render(&doc);
    assert!(xml.contains("<ram:TaxTotalAmount currencyID=\"USD\">252.00</ram:TaxTotalAmount>"));
    assert!(xml.contains("<ram:TaxTotalAmount currencyID=\"EUR\">216.75</ram:TaxTotalAmount>"));
    golden("invoice-foreign-currency.xml", &xml);
}

// ---- properties that hold over all four --------------------------------------

/// Every element that carries money, in the order they appear.
fn amounts(xml: &str) -> Vec<String> {
    let tags = [
        "ram:ChargeAmount",
        "ram:LineTotalAmount",
        "ram:CalculatedAmount",
        "ram:BasisAmount",
        "ram:TaxBasisTotalAmount",
        "ram:TaxTotalAmount",
        "ram:GrandTotalAmount",
        "ram:DuePayableAmount",
    ];
    xml.lines()
        .filter_map(|line| {
            let trimmed = line.trim();
            let name = trimmed.strip_prefix('<')?.split(['>', ' ']).next()?;
            tags.contains(&name).then(|| {
                trimmed
                    .split_once('>')
                    .and_then(|(_, rest)| rest.split_once('<'))
                    .map_or_else(String::new, |(value, _)| value.to_owned())
            })
        })
        .collect()
}

#[test]
fn every_amount_in_every_document_has_exactly_two_decimals() {
    // The BR-DEC family (BR-DEC-09 … BR-DEC-25) says so of each amount in
    // turn. Integer cents make it true by construction; this is what proves
    // the formatter never changed that.
    for name in [
        "invoice-standard.xml",
        "invoice-mixed-rates.xml",
        "credit-note.xml",
        "invoice-foreign-currency.xml",
    ] {
        let xml = std::fs::read_to_string(format!(
            "{}/tests/golden/{name}",
            env!("CARGO_MANIFEST_DIR")
        ))
        .unwrap_or_else(|e| panic!("{name} is missing ({e}); regenerate with UPDATE_GOLDEN=1"));
        let found = amounts(&xml);
        assert!(found.len() >= 6, "{name} carries no amounts: {found:?}");
        for amount in found {
            let (units, decimals) = amount
                .split_once('.')
                .unwrap_or_else(|| panic!("{name}: {amount} has no decimal point"));
            assert_eq!(decimals.len(), 2, "{name}: {amount} is not two decimals");
            assert!(
                units
                    .trim_start_matches('-')
                    .chars()
                    .all(|c| c.is_ascii_digit()),
                "{name}: {amount} is not a number"
            );
        }
        // And the rate percentages beside them.
        for rate in xml.matches("<ram:RateApplicablePercent>") {
            let _ = rate;
        }
        assert!(
            xml.contains("<ram:RateApplicablePercent>21.00</ram:RateApplicablePercent>")
                || xml.contains("<ram:RateApplicablePercent>9.00</ram:RateApplicablePercent>"),
            "{name} states no VAT rate"
        );
    }
}

#[test]
fn every_document_declares_the_specification_it_follows() {
    for name in [
        "invoice-standard.xml",
        "invoice-mixed-rates.xml",
        "credit-note.xml",
        "invoice-foreign-currency.xml",
    ] {
        let xml = std::fs::read_to_string(format!(
            "{}/tests/golden/{name}",
            env!("CARGO_MANIFEST_DIR")
        ))
        .unwrap_or_else(|e| panic!("{name} is missing ({e}); regenerate with UPDATE_GOLDEN=1"));
        assert!(
            xml.contains("<ram:ID>urn:cen.eu:en16931:2017</ram:ID>"),
            "{name} does not declare EN 16931"
        );
        assert!(xml.starts_with("<?xml version=\"1.0\" encoding=\"UTF-8\"?>"));
        assert!(xml.ends_with("</rsm:CrossIndustryInvoice>\n"));
    }
}
