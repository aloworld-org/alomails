//! Golden files for the XRechnung e-invoice (B1.23): the same four documents
//! `billing_cii_golden` pins in the other syntax, byte for byte.
//!
//! Reading the two golden sets side by side is the point. They are the *same
//! invoices* — same customer, same lines, same money — and they share almost no
//! bytes: UBL puts the lines last and CII puts them first, UBL states a
//! currency on every amount and CII on exactly one, UBL splits a credit note
//! into a different root schema and CII changes a code. If a change to
//! `billing_einvoice.rs` moves a figure, both sets move together; if only one
//! moves, a syntax has drifted from the model and the diff says which.
//!
//! | File | What it pins |
//! |---|---|
//! | `xrechnung-standard.xml` | the everyday document: one rate, bank details, a reference, a note |
//! | `xrechnung-mixed-rates.xml` | three rates including 0 % (categories `S` and `Z`), several units, a two-paragraph line, no bank account, and a real Leitweg-ID |
//! | `xrechnung-credit-note.xml` | the credit-note *schema*: `ubl:CreditNote`, `cbc:CreditedQuantity`, positive amounts, no due date, no account |
//! | `xrechnung-foreign-currency.xml` | BT-6/BT-111: a USD document whose VAT is restated in EUR in a second `cac:TaxTotal` |
//!
//! Every one is run through **both** rule sets first — the European
//! (`billing_einvoice_rules`) and the German (`billing_xrechnung_rules`) —
//! because a golden file of a document a gateway would refuse pins a mistake.
//!
//! To regenerate after an intended change: `UPDATE_GOLDEN=1 cargo test -p
//! alo-jmap --test billing_ubl_golden`, then read the diff before committing it.
//!
//! **What these are not.** They are our own output, not the KoSIT test suite,
//! and the checker they pass is our hand-written subset rather than the
//! normative schematron — which is XSLT, i.e. a third language and a downloaded
//! artefact in a public repository (`CLAUDE.md`). Running the real validator
//! over these files once, offline, stays an open item for a human, recorded in
//! `docs/autonomy/STATE.md`; these files are what makes that a one-off check
//! rather than a standing risk.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use time::{Date, Month, OffsetDateTime};

use alo_jmap::billing_einvoice::EInvoice;
use alo_jmap::billing_einvoice_rules::violations;
use alo_jmap::billing_print::{DocumentKind, Party, PrintDocument, Restated, strings_for};
use alo_jmap::billing_ubl as ubl;
use alo_jmap::billing_xrechnung_rules;
use alo_store::billing_settings::BillingSettings;
use alo_store::billing_totals::{LineFigures, Totals, totals};
use alo_store::{BillingCustomerId, BillingLineId, Customer, Line};

/// Every golden document in this file, for the properties that hold over all
/// of them.
const GOLDEN: [&str; 4] = [
    "xrechnung-standard.xml",
    "xrechnung-mixed-rates.xml",
    "xrechnung-credit-note.xml",
    "xrechnung-foreign-currency.xml",
];

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

/// The issuer XRechnung asks for: everything EN 16931 wants, plus the contact
/// telephone (BT-42) the German CIUS makes mandatory.
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
        phone: "+31 20 123 4567".to_owned(),
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

/// The XML of a document, checked against **both** rule sets before it is
/// compared: the European standard's, then the German narrowing of it.
fn render(doc: &PrintDocument<'_>) -> String {
    let einvoice = EInvoice::from_document(doc, strings_for("en")).expect("an invoice document");
    let mut broken = violations(&einvoice);
    broken.extend(billing_xrechnung_rules::violations(&einvoice));
    assert!(
        broken.is_empty(),
        "the fixture itself would be refused: {broken:?}"
    );
    ubl::render(&einvoice)
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

fn stored(name: &str) -> String {
    std::fs::read_to_string(format!(
        "{}/tests/golden/{name}",
        env!("CARGO_MANIFEST_DIR")
    ))
    .unwrap_or_else(|e| panic!("{name} is missing ({e}); regenerate with UPDATE_GOLDEN=1"))
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
    // The same figures `billing_cii_golden` pins, on the wire UBL reads them
    // off: 15 h at 125.00 is 1875.00, 240 km at 0.42 is 100.80, and 21 % of
    // 1975.80 rounds once at the rate subtotal to 414.92.
    assert!(
        xml.contains(
            "<cbc:LineExtensionAmount currencyID=\"EUR\">1875.00</cbc:LineExtensionAmount>"
        )
    );
    assert!(
        xml.contains(
            "<cbc:LineExtensionAmount currencyID=\"EUR\">100.80</cbc:LineExtensionAmount>"
        )
    );
    assert!(xml.contains("<cbc:TaxAmount currencyID=\"EUR\">414.92</cbc:TaxAmount>"));
    assert!(xml.contains("<cbc:PayableAmount currencyID=\"EUR\">2390.72</cbc:PayableAmount>"));
    // The dates are ISO here and `20260807` in the CII file: the same day.
    assert!(xml.contains("<cbc:IssueDate>2026-08-07</cbc:IssueDate>"));
    assert!(xml.contains("<cbc:DueDate>2026-08-21</cbc:DueDate>"));
    // And the money goes somewhere, by credit transfer.
    assert!(xml.contains("<cbc:PaymentMeansCode>30</cbc:PaymentMeansCode>"));
    assert!(xml.contains("<cbc:ID>NL91ABNA0417164300</cbc:ID>"));
    assert!(xml.contains("<cbc:ID>ABNANL2A</cbc:ID>"));
    golden("xrechnung-standard.xml", &xml);
}

#[test]
fn three_rates_two_categories_and_a_leitweg_id() {
    let (customer, mut issuer) = (customer(), issuer());
    // No bank account stated: the payment-means group stays (XRechnung requires
    // it) and says the instrument is not defined rather than inventing one.
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
        // The routing identifier a German authority is addressed by, which is
        // what BT-10 carries in the public-sector case XRechnung exists for.
        reference: "04011000-12345-06",
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
    assert!(xml.contains("<cbc:BuyerReference>04011000-12345-06</cbc:BuyerReference>"));
    // Three rates, three subtotals, each with a category and a percentage; the
    // 0 % one is category Z and carries no VAT.
    assert_eq!(xml.matches("<cac:TaxSubtotal>").count(), 3);
    assert!(xml.contains("<cbc:ID>Z</cbc:ID>"));
    assert!(xml.contains("<cbc:Percent>0.00</cbc:Percent>"));
    assert!(xml.contains("<cbc:InvoicedQuantity unitCode=\"DAY\">4</cbc:InvoicedQuantity>"));
    assert!(xml.contains("<cbc:InvoicedQuantity unitCode=\"H87\">12</cbc:InvoicedQuantity>"));
    assert!(xml.contains("<cbc:InvoicedQuantity unitCode=\"C62\">1</cbc:InvoicedQuantity>"));
    // A due date it does not have is not invented; the terms say it in words.
    assert!(!xml.contains("<cbc:DueDate>"));
    assert!(xml.contains("<cbc:Note>Payable within 30 days"));
    // The paragraph under the item name is the item's description, not its name.
    assert!(
        xml.contains(
            "<cbc:Description>March, on site. Two engineers, four days.</cbc:Description>"
        )
    );
    assert!(xml.contains("<cbc:PaymentMeansCode>1</cbc:PaymentMeansCode>"));
    assert!(!xml.contains("PayeeFinancialAccount"));
    golden("xrechnung-mixed-rates.xml", &xml);
}

#[test]
fn the_credit_note_is_written_in_the_credit_note_schema() {
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
    // Not a code on an invoice — a different root schema, a different line
    // element and a different quantity element.
    assert!(xml.contains("<ubl:CreditNote "));
    assert!(xml.contains("schema:xsd:CreditNote-2"));
    assert!(xml.contains("<cbc:CreditNoteTypeCode>381</cbc:CreditNoteTypeCode>"));
    assert!(xml.contains("<cbc:CreditedQuantity unitCode=\"HUR\">15</cbc:CreditedQuantity>"));
    // Positive on the wire, negative in our ledger: direction is the type
    // code's job, and nothing here is negative.
    assert!(xml.contains("<cbc:PayableAmount currencyID=\"EUR\">2268.75</cbc:PayableAmount>"));
    assert!(!xml.contains(">-"));
    assert!(xml.contains("<cbc:ID>INV-2026-00001</cbc:ID>"));
    // Nothing is payable on it: no due date, no account, no reference to quote.
    assert!(!xml.contains("<cbc:DueDate>"));
    assert!(!xml.contains("PayeeFinancialAccount"));
    assert!(!xml.contains("<cbc:PaymentID>"));
    golden("xrechnung-credit-note.xml", &xml);
}

#[test]
fn a_document_in_another_currency_states_its_vat_in_a_second_total() {
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
    assert!(xml.contains("<cbc:DocumentCurrencyCode>USD</cbc:DocumentCurrencyCode>"));
    assert!(xml.contains("<cbc:TaxCurrencyCode>EUR</cbc:TaxCurrencyCode>"));
    assert!(xml.contains("<cbc:TaxAmount currencyID=\"USD\">252.00</cbc:TaxAmount>"));
    assert!(xml.contains("<cbc:TaxAmount currencyID=\"EUR\">216.75</cbc:TaxAmount>"));
    // Two totals, one breakdown: the subtotals belong to the currency the
    // document was raised in.
    assert_eq!(xml.matches("<cac:TaxTotal>").count(), 2);
    assert_eq!(xml.matches("<cac:TaxSubtotal>").count(), 1);
    golden("xrechnung-foreign-currency.xml", &xml);
}

// ---- properties that hold over all four --------------------------------------

/// Every element that carries money, as `(element, currency, value)`.
fn amounts(xml: &str) -> Vec<(String, String, String)> {
    let tags = [
        "cbc:LineExtensionAmount",
        "cbc:TaxableAmount",
        "cbc:TaxAmount",
        "cbc:TaxExclusiveAmount",
        "cbc:TaxInclusiveAmount",
        "cbc:PayableAmount",
        "cbc:PriceAmount",
    ];
    xml.lines()
        .filter_map(|line| {
            let trimmed = line.trim();
            let name = trimmed.strip_prefix('<')?.split(['>', ' ']).next()?;
            if !tags.contains(&name) {
                return None;
            }
            let currency = trimmed
                .split_once("currencyID=\"")
                .and_then(|(_, rest)| rest.split_once('"'))
                .map_or_else(String::new, |(code, _)| code.to_owned());
            let value = trimmed
                .split_once('>')
                .and_then(|(_, rest)| rest.split_once('<'))
                .map_or_else(String::new, |(value, _)| value.to_owned());
            Some((name.to_owned(), currency, value))
        })
        .collect()
}

#[test]
fn every_amount_states_a_currency_and_exactly_two_decimals() {
    // Two rule families at once: the BR-DEC family (each amount is two
    // decimals), which integer cents make true by construction, and UBL's own
    // requirement that an amount without a `currencyID` is not an amount.
    for name in GOLDEN {
        let xml = stored(name);
        let found = amounts(&xml);
        assert!(found.len() >= 6, "{name} carries no amounts: {found:?}");
        for (tag, currency, value) in found {
            assert_eq!(currency.len(), 3, "{name}: {tag} states no currency");
            assert!(
                currency.bytes().all(|b| b.is_ascii_uppercase()),
                "{name}: {tag} states {currency}"
            );
            let (units, decimals) = value
                .split_once('.')
                .unwrap_or_else(|| panic!("{name}: {tag} is {value}, which has no decimal point"));
            assert_eq!(decimals.len(), 2, "{name}: {value} is not two decimals");
            assert!(
                units
                    .trim_start_matches('-')
                    .chars()
                    .all(|c| c.is_ascii_digit()),
                "{name}: {value} is not a number"
            );
        }
    }
}

#[test]
fn every_document_declares_the_specification_it_follows() {
    for name in GOLDEN {
        let xml = stored(name);
        assert!(
            xml.contains(&format!(
                "<cbc:CustomizationID>{}</cbc:CustomizationID>",
                ubl::CUSTOMIZATION_ID
            )),
            "{name} does not declare XRechnung"
        );
        assert!(
            xml.contains(&format!(
                "<cbc:ProfileID>{}</cbc:ProfileID>",
                ubl::PROFILE_ID
            )),
            "{name} does not declare the billing process"
        );
        assert!(xml.starts_with("<?xml version=\"1.0\" encoding=\"UTF-8\"?>"));
        // Both namespaces, on the root, spelled the way a validator resolves.
        assert!(
            xml.contains(
                "xmlns:cac=\"urn:oasis:names:specification:ubl:schema:xsd:CommonAggregateComponents-2\""
            ),
            "{name} does not bind cac"
        );
        assert!(
            xml.contains(
                "xmlns:cbc=\"urn:oasis:names:specification:ubl:schema:xsd:CommonBasicComponents-2\""
            ),
            "{name} does not bind cbc"
        );
    }
}

#[test]
fn every_document_names_both_parties_and_a_reference_to_route_it_by() {
    // The three XRechnung asks for that EN 16931 does not, proven on the files
    // rather than only in the checker's own tests.
    for name in GOLDEN {
        let xml = stored(name);
        assert!(
            xml.contains("<cbc:BuyerReference>"),
            "{name} has no buyer reference"
        );
        assert!(
            xml.contains("<cbc:Telephone>+31 20 123 4567</cbc:Telephone>"),
            "{name} names no telephone for the seller"
        );
        assert!(
            xml.contains("<cbc:ElectronicMail>billing@alo.test</cbc:ElectronicMail>"),
            "{name} names no email for the seller"
        );
        // One contact desk: the seller's. The customer's address is its
        // electronic address, not a contact person.
        assert_eq!(
            xml.matches("<cac:Contact>").count(),
            1,
            "{name} states the wrong number of contacts"
        );
        for postal in ["<cbc:PostalZone>1015 CJ", "<cbc:PostalZone>10115"] {
            assert!(xml.contains(postal), "{name} is missing {postal}");
        }
    }
}
