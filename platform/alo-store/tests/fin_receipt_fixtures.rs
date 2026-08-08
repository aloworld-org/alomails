//! Seven real-shaped European receipts, read field by field (B4.06a).
//!
//! The unit tests beside [`alo_store::fin_receipt`] prove each rule on the one
//! line it is about. These prove the whole extractor on documents shaped like
//! the paper people actually photograph: a Munich till roll, a Leipzig hotel
//! folio with two VAT rates, an Amsterdam supermarket, a Paris bistro, a Leeds
//! taxi, a parking ticket with nothing on it but a number, and the text layer
//! of a German supplier invoice.
//!
//! They are also the **contract an AI backend must meet**. The extractor is a
//! trait ([`ReceiptExtractor`]) with one implementation today; the day a human
//! wires a second one (ADR 0029, EU-only inference), this file is what says
//! whether it reads a receipt as well as the patterns do. Which is why every
//! expectation below is written as a *field of the document*, never as an
//! artefact of how the pattern extractor happens to work.
//!
//! Two things no fixture may ever assert, because the module must never do
//! them: a VAT amount on a receipt that prints only a rate, and a total the
//! receipt does not print. Both are covered by their own negative cases below.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use alo_store::fin_receipt::{
    Confidence, Evidence, Found, ParsedReceipt, PatternExtractor, ReceiptExtractor, ReceiptInput,
};
use time::{Date, Month};

/// The day these readings happen on. Fixed, so a test that passes in March
/// still passes in April — the extractor takes "today" as an argument for
/// exactly this reason.
fn today() -> Date {
    day(2026, Month::June, 30)
}

fn day(year: i32, month: Month, day: u8) -> Date {
    Date::from_calendar_date(year, month, day).expect("a real day")
}

fn read(fixture: &str) -> ParsedReceipt {
    let text = match fixture {
        "rewe_de" => include_str!("fixtures/receipts/rewe_de.txt"),
        "hotel_folio_de" => include_str!("fixtures/receipts/hotel_folio_de.txt"),
        "albert_heijn_nl" => include_str!("fixtures/receipts/albert_heijn_nl.txt"),
        "bistro_fr" => include_str!("fixtures/receipts/bistro_fr.txt"),
        "taxi_uk_en" => include_str!("fixtures/receipts/taxi_uk_en.txt"),
        "parkhaus_de" => include_str!("fixtures/receipts/parkhaus_de.txt"),
        "bueromeyer_invoice_de" => include_str!("fixtures/receipts/bueromeyer_invoice_de.txt"),
        other => panic!("no such fixture: {other}"),
    };
    PatternExtractor.extract(&ReceiptInput {
        text,
        filename: Some(&format!("{fixture}.txt")),
        today: today(),
    })
}

/// The value of a field that must be there, with the fixture named in the
/// failure so a broken reading says which document broke.
fn value<T: Clone>(field: Option<&Found<T>>, what: &str, fixture: &str) -> T {
    field
        .unwrap_or_else(|| panic!("{fixture}: no {what} was read"))
        .value
        .clone()
}

/// Every span a reading came from points at the characters it claims to.
fn evidence_holds(parsed: &ParsedReceipt, fixture: &str) {
    let spans = [
        parsed.merchant.as_ref().map(|found| found.evidence.clone()),
        parsed.spent_on.as_ref().map(|found| found.evidence.clone()),
        parsed
            .gross_cents
            .as_ref()
            .map(|found| found.evidence.clone()),
        parsed
            .vat_cents
            .as_ref()
            .map(|found| found.evidence.clone()),
        parsed
            .vat_rate_bp
            .as_ref()
            .map(|found| found.evidence.clone()),
        parsed.currency.as_ref().map(|found| found.evidence.clone()),
    ];
    for evidence in spans.into_iter().flatten() {
        let Evidence::Text { line, start, end } = evidence else {
            continue;
        };
        let text = parsed.lines.get(line).unwrap_or_else(|| {
            panic!("{fixture}: evidence points at line {line}, which is not there")
        });
        let length = text.chars().count();
        assert!(
            start < end && end <= length,
            "{fixture}: evidence {start}..{end} is outside a line of {length} characters"
        );
    }
}

#[test]
fn a_munich_till_roll() {
    let parsed = read("rewe_de");
    assert_eq!(
        value(parsed.merchant.as_ref(), "merchant", "rewe_de"),
        "REWE Markt GmbH"
    );
    assert_eq!(
        parsed.merchant.as_ref().map(|found| found.confidence),
        Some(Confidence::High),
        "a legal form is not a guess"
    );
    assert_eq!(
        value(parsed.spent_on.as_ref(), "date", "rewe_de"),
        day(2026, Month::March, 14)
    );
    assert_eq!(value(parsed.gross_cents.as_ref(), "total", "rewe_de"), 1190);
    assert_eq!(
        value(parsed.vat_cents.as_ref(), "VAT", "rewe_de"),
        78,
        "the tax printed in the MwSt table, not the net or the gross beside it"
    );
    assert_eq!(value(parsed.vat_rate_bp.as_ref(), "rate", "rewe_de"), 700);
    assert_eq!(
        value(parsed.currency.as_ref(), "currency", "rewe_de"),
        "EUR"
    );
    evidence_holds(&parsed, "rewe_de");
}

#[test]
fn a_hotel_folio_with_two_rates_states_a_tax_total_and_no_rate() {
    let parsed = read("hotel_folio_de");
    assert_eq!(
        value(parsed.merchant.as_ref(), "merchant", "hotel_folio_de"),
        "Hotel Adler Betriebs GmbH"
    );
    assert_eq!(
        value(parsed.spent_on.as_ref(), "date", "hotel_folio_de"),
        day(2026, Month::March, 12),
        "the invoice date, which is the labelled one"
    );
    assert_eq!(
        value(parsed.gross_cents.as_ref(), "total", "hotel_folio_de"),
        24_000
    );
    assert_eq!(
        value(parsed.vat_cents.as_ref(), "VAT", "hotel_folio_de"),
        1321 + 607,
        "both printed taxes, added — each one is a fact on the paper"
    );
    assert!(
        parsed.vat_rate_bp.is_none(),
        "7% on the room and 19% on dinner is not one rate"
    );
    evidence_holds(&parsed, "hotel_folio_de");
}

#[test]
fn an_amsterdam_supermarket() {
    let parsed = read("albert_heijn_nl");
    assert_eq!(
        value(parsed.merchant.as_ref(), "merchant", "albert_heijn_nl"),
        "Albert Heijn 1234"
    );
    assert_eq!(
        value(parsed.spent_on.as_ref(), "date", "albert_heijn_nl"),
        day(2026, Month::March, 14)
    );
    assert_eq!(
        value(parsed.gross_cents.as_ref(), "total", "albert_heijn_nl"),
        2415,
        "TOTAAL, and never SUBTOTAAL"
    );
    assert_eq!(
        value(parsed.vat_cents.as_ref(), "VAT", "albert_heijn_nl"),
        199
    );
    assert_eq!(
        value(parsed.vat_rate_bp.as_ref(), "rate", "albert_heijn_nl"),
        900
    );
    evidence_holds(&parsed, "albert_heijn_nl");
}

#[test]
fn a_paris_bistro_prefers_ttc_over_ht() {
    let parsed = read("bistro_fr");
    assert_eq!(
        value(parsed.merchant.as_ref(), "merchant", "bistro_fr"),
        "LE PETIT BISTROT SARL"
    );
    assert_eq!(
        value(parsed.spent_on.as_ref(), "date", "bistro_fr"),
        day(2026, Month::March, 14),
        "14/03 can only be the fourteenth"
    );
    assert_eq!(
        value(parsed.gross_cents.as_ref(), "total", "bistro_fr"),
        5555,
        "Montant TTC is what was paid; Total HT is not"
    );
    assert_eq!(value(parsed.vat_cents.as_ref(), "VAT", "bistro_fr"), 505);
    assert_eq!(
        value(parsed.vat_rate_bp.as_ref(), "rate", "bistro_fr"),
        1000
    );
    assert_eq!(
        value(parsed.currency.as_ref(), "currency", "bistro_fr"),
        "EUR"
    );
    evidence_holds(&parsed, "bistro_fr");
}

#[test]
fn a_leeds_taxi_in_pounds_never_reads_its_registration_number_as_tax() {
    let parsed = read("taxi_uk_en");
    assert_eq!(
        value(parsed.merchant.as_ref(), "merchant", "taxi_uk_en"),
        "CITY CABS LIMITED"
    );
    assert_eq!(
        value(parsed.spent_on.as_ref(), "date", "taxi_uk_en"),
        day(2026, Month::March, 11)
    );
    assert_eq!(
        value(parsed.gross_cents.as_ref(), "total", "taxi_uk_en"),
        2810
    );
    assert_eq!(
        value(parsed.vat_cents.as_ref(), "VAT", "taxi_uk_en"),
        468,
        "the VAT line, not the four digit groups of GB 123 4567 89"
    );
    assert_eq!(
        value(parsed.vat_rate_bp.as_ref(), "rate", "taxi_uk_en"),
        2000
    );
    assert_eq!(
        value(parsed.currency.as_ref(), "currency", "taxi_uk_en"),
        "GBP"
    );
    evidence_holds(&parsed, "taxi_uk_en");
}

#[test]
fn a_parking_ticket_with_one_number_on_it() {
    let parsed = read("parkhaus_de");
    assert_eq!(
        value(parsed.merchant.as_ref(), "merchant", "parkhaus_de"),
        "PARKHAUS AM DOM"
    );
    assert_eq!(
        parsed.merchant.as_ref().map(|found| found.confidence),
        Some(Confidence::Medium),
        "no legal form, so the name is the best line rather than a certainty"
    );
    assert_eq!(
        value(parsed.spent_on.as_ref(), "date", "parkhaus_de"),
        day(2026, Month::March, 14),
        "unlabelled, but the first plausible day on the ticket"
    );
    assert_eq!(
        value(parsed.gross_cents.as_ref(), "total", "parkhaus_de"),
        450,
        "the only amount printed with cents — 2 Std and 35 Min are not money"
    );
    assert_eq!(
        parsed.gross_cents.as_ref().map(|found| found.confidence),
        Some(Confidence::Low),
        "nothing labelled it, so it is offered for correction"
    );
    assert!(
        parsed.vat_cents.is_none() && parsed.vat_rate_bp.is_none(),
        "the ticket shows no tax, so the claim reclaims none"
    );
    evidence_holds(&parsed, "parkhaus_de");
}

#[test]
fn the_text_layer_of_a_supplier_invoice() {
    let parsed = read("bueromeyer_invoice_de");
    assert_eq!(
        value(
            parsed.merchant.as_ref(),
            "merchant",
            "bueromeyer_invoice_de"
        ),
        "Bürobedarf Meyer GmbH & Co. KG"
    );
    assert_eq!(
        value(parsed.spent_on.as_ref(), "date", "bueromeyer_invoice_de"),
        day(2026, Month::March, 2),
        "the invoice date, which is printed before the delivery date"
    );
    assert_eq!(
        value(
            parsed.gross_cents.as_ref(),
            "total",
            "bueromeyer_invoice_de"
        ),
        33_879,
        "the Rechnungsbetrag, never the Zwischensumme netto"
    );
    assert_eq!(
        value(parsed.vat_cents.as_ref(), "VAT", "bueromeyer_invoice_de"),
        5409
    );
    assert_eq!(
        value(parsed.vat_rate_bp.as_ref(), "rate", "bueromeyer_invoice_de"),
        1900
    );
    assert_eq!(
        value(
            parsed.currency.as_ref(),
            "currency",
            "bueromeyer_invoice_de"
        ),
        "EUR"
    );
    evidence_holds(&parsed, "bueromeyer_invoice_de");
}

#[test]
fn no_fixture_ever_yields_a_tax_the_paper_did_not_print() {
    // The rule this module exists to keep: on every document we read, the tax
    // is either printed or absent — it is never `gross × rate / (1 + rate)`.
    for fixture in [
        "rewe_de",
        "hotel_folio_de",
        "albert_heijn_nl",
        "bistro_fr",
        "taxi_uk_en",
        "parkhaus_de",
        "bueromeyer_invoice_de",
    ] {
        let parsed = read(fixture);
        let Some(vat) = parsed.vat_cents.as_ref() else {
            continue;
        };
        let Evidence::Text { line, start, end } = vat.evidence else {
            panic!("{fixture}: a tax amount can only come from the text");
        };
        let printed: String = parsed.lines[line]
            .chars()
            .skip(start)
            .take(end - start)
            .collect();
        let cents: i64 = printed
            .replace(['.', ','], "")
            .parse()
            .unwrap_or_else(|_| panic!("{fixture}: {printed:?} is not the amount it points at"));
        // Either the span is the whole tax (one rate) or the tax is a sum of
        // printed amounts of which this is the first (several rates).
        assert!(
            cents == vat.value || (parsed.vat_rate_bp.is_none() && cents < vat.value),
            "{fixture}: the tax {} does not correspond to the printed {printed:?}",
            vat.value
        );
    }
}

#[test]
fn every_fixture_is_read_without_reading_anything_into_it() {
    for fixture in [
        "rewe_de",
        "hotel_folio_de",
        "albert_heijn_nl",
        "bistro_fr",
        "taxi_uk_en",
        "parkhaus_de",
        "bueromeyer_invoice_de",
    ] {
        let parsed = read(fixture);
        assert!(
            parsed.found_anything(),
            "{fixture}: nothing was read at all"
        );
        // A total is money on the receipt, not a line number or a year.
        let gross = value(parsed.gross_cents.as_ref(), "total", fixture);
        assert!(
            gross > 0 && gross < 1_000_000,
            "{fixture}: {gross} is not a receipt total"
        );
        if let Some(vat) = parsed.vat_cents.as_ref() {
            assert!(
                vat.value < gross,
                "{fixture}: the tax cannot exceed what was paid"
            );
        }
        evidence_holds(&parsed, fixture);
    }
}
