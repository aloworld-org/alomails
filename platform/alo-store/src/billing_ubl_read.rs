//! Reading OASIS UBL 2.1 — the syntax XRechnung and Peppol carry (alo Billing,
//! ADR 0035, wave B1.24).
//!
//! The sibling of [`crate::billing_cii_read`], and the reason the semantic
//! model ([`crate::billing_einvoice_import`]) is a module of its own: this file
//! knows where UBL writes things down and nothing else. Everything it decides
//! about *the invoice* — that a 381 is a credit note, that a category we cannot
//! express is a refusal, that the totals must add up — is decided once, over
//! there.
//!
//! Three things are genuinely different from CII, and all three are syntax:
//!
//! - **A credit note is a different root element**, not a different code:
//!   `CreditNote` with `cac:CreditNoteLine` and `cbc:CreditedQuantity`, where
//!   CII keeps one document shape and changes `TypeCode`. The caller has
//!   already read the root, which is why `credit_note` arrives as an argument.
//! - **Every amount states its `currencyID`**, so the VAT restated in a second
//!   accounting currency (BT-111) is told from BT-110 by that attribute rather
//!   than by position.
//! - **The seller's name is the registered one** (`cac:PartyLegalEntity`), with
//!   the trading name (`cac:PartyName`) as the fallback — UBL states both, CII
//!   states one.

use crate::billing_einvoice_import::{
    EInvoiceSyntax, InboundInvoice, InboundLine, InboundParty, StatedTotals, amount, category,
    date, describe, quantity, rate_bp, unit_label,
};
use crate::billing_xml_tree::Element;
use crate::error::{Result, StoreError};

/// Reads a UBL `Invoice` or `CreditNote` into the semantic model.
///
/// `credit_note` comes from the root element the caller matched on.
///
/// # Errors
/// [`StoreError::Validation`] when a mandatory term is missing or a value
/// cannot be held in our units. Messages name the business term, never the
/// value.
pub(crate) fn read(root: &Element, credit_note: bool) -> Result<InboundInvoice> {
    // The type code is stated as well as implied by the root; when it is there
    // it must agree, because a `CreditNote` that says 380 is a document whose
    // own two halves disagree about which way the money goes.
    let stated_type = if credit_note {
        root.text_at(&["CreditNoteTypeCode"])
    } else {
        root.text_at(&["InvoiceTypeCode"])
    };
    check_type_code(stated_type, credit_note)?;

    let currency = root.text_at(&["DocumentCurrencyCode"]).to_uppercase();
    let monetary = root.at(&["LegalMonetaryTotal"]).ok_or_else(|| {
        StoreError::Validation(
            "BG-22: this document states no LegalMonetaryTotal, so it says nothing about what it \
             is worth"
                .to_owned(),
        )
    })?;

    let line_element = if credit_note {
        "CreditNoteLine"
    } else {
        "InvoiceLine"
    };
    let mut lines = Vec::new();
    for (index, element) in root.children_named(line_element).enumerate() {
        lines.push(line(index + 1, element, credit_note)?);
    }

    Ok(InboundInvoice {
        syntax: EInvoiceSyntax::Ubl,
        credit_note,
        number: root.text_at(&["ID"]).to_owned(),
        issue_date: date("BT-2", root.text_at(&["IssueDate"]))?,
        due_date: optional_date("BT-9", root.text_at(&["DueDate"]))?,
        buyer_reference: root.text_at(&["BuyerReference"]).to_owned(),
        note: root.text_at(&["Note"]).to_owned(),
        payment_reference: root.text_at(&["PaymentMeans", "PaymentID"]).to_owned(),
        iban: root
            .text_at(&["PaymentMeans", "PayeeFinancialAccount", "ID"])
            .to_owned(),
        seller: root
            .at(&["AccountingSupplierParty", "Party"])
            .map(party)
            .unwrap_or_default(),
        lines,
        totals: StatedTotals {
            line_total_cents: amount("BT-106", monetary.text_at(&["LineExtensionAmount"]))?,
            allowance_total_cents: optional_amount(
                "BT-107",
                monetary.text_at(&["AllowanceTotalAmount"]),
            )?,
            charge_total_cents: optional_amount(
                "BT-108",
                monetary.text_at(&["ChargeTotalAmount"]),
            )?,
            tax_exclusive_cents: amount("BT-109", monetary.text_at(&["TaxExclusiveAmount"]))?,
            tax_total_cents: amount("BT-110", tax_total(root, &currency))?,
            tax_inclusive_cents: amount("BT-112", monetary.text_at(&["TaxInclusiveAmount"]))?,
            prepaid_cents: optional_amount("BT-113", monetary.text_at(&["PrepaidAmount"]))?,
            rounding_cents: optional_amount(
                "BT-114",
                monetary.text_at(&["PayableRoundingAmount"]),
            )?,
            payable_cents: amount("BT-115", monetary.text_at(&["PayableAmount"]))?,
        },
        currency,
    })
}

/// Checks the stated type code (BT-3) against the root element the document
/// chose.
///
/// An absent code is accepted: the root element already says which document
/// this is, and that is the half UBL cannot get wrong.
fn check_type_code(code: &str, credit_note: bool) -> Result<()> {
    let expected = if credit_note { "381" } else { "380" };
    match code.trim() {
        "" => Ok(()),
        stated if stated == expected => Ok(()),
        "380" | "381" => Err(StoreError::Validation(format!(
            "BT-3: the document states type {}, which contradicts the {} it is written as",
            code.trim(),
            if credit_note {
                "credit note"
            } else {
                "invoice"
            }
        ))),
        other => Err(StoreError::Validation(format!(
            "BT-3: document type {other} is not one alo can store. Only a commercial invoice (380) \
             and a credit note (381) can be booked as a bill"
        ))),
    }
}

/// A party (BG-4) as UBL states it.
fn party(element: &Element) -> InboundParty {
    let address = element.at(&["PostalAddress"]);
    let text = |path: &[&str]| address.map_or("", |a| a.text_at(path)).to_owned();
    let legal = element.at(&["PartyLegalEntity"]);
    let registered = legal.map_or("", |l| l.text_at(&["RegistrationName"]));
    let trading = element.text_at(&["PartyName", "Name"]);
    InboundParty {
        // BT-27 is the registered name; a supplier who states only a trading
        // name (BT-28) is still named, and naming them is better than a blank.
        name: if registered.is_empty() {
            trading.to_owned()
        } else {
            registered.to_owned()
        },
        // A party may state several tax schemes (VAT and a national one); the
        // VAT scheme is the one BT-31 lives in.
        vat_id: element
            .children_named("PartyTaxScheme")
            .find(|scheme| {
                scheme
                    .text_at(&["TaxScheme", "ID"])
                    .eq_ignore_ascii_case("VAT")
            })
            .or_else(|| element.child("PartyTaxScheme"))
            .map(|scheme| scheme.text_at(&["CompanyID"]).to_owned())
            .unwrap_or_default(),
        legal_id: legal.map_or("", |l| l.text_at(&["CompanyID"])).to_owned(),
        line1: text(&["StreetName"]),
        line2: text(&["AdditionalStreetName"]),
        postal_code: text(&["PostalZone"]),
        city: text(&["CityName"]),
        country: text(&["Country", "IdentificationCode"]).to_uppercase(),
        email: email(element),
    }
}

/// The party's email address (BT-34/BT-49 when the endpoint is one, otherwise
/// the contact's).
fn email(element: &Element) -> String {
    let endpoint = element
        .child("EndpointID")
        .filter(|endpoint| endpoint.attr("schemeID").eq_ignore_ascii_case("EM"));
    match endpoint {
        Some(endpoint) => endpoint.text.clone(),
        None => element.text_at(&["Contact", "ElectronicMail"]).to_owned(),
    }
}

/// One line (BG-25) as UBL states it.
fn line(position: usize, element: &Element, credit_note: bool) -> Result<InboundLine> {
    let quantity_element = if credit_note {
        "CreditedQuantity"
    } else {
        "InvoicedQuantity"
    };
    let billed = element.child(quantity_element).ok_or_else(|| {
        StoreError::Validation(format!(
            "line {position}: BT-129 — the line states no quantity"
        ))
    })?;
    let price = element.at(&["Price"]).ok_or_else(|| {
        StoreError::Validation(format!(
            "line {position}: BT-146 — the line states no net price"
        ))
    })?;
    // A price stated per a base quantity ("€80.00 per 100 pieces") multiplies
    // the whole line, and our line model holds one price per one unit.
    if let Some(base) = price.child("BaseQuantity") {
        let base = quantity("BT-149", &base.text)?;
        if base != 1_000 {
            return Err(StoreError::Validation(format!(
                "line {position}: BT-149 — the price is stated per a base quantity other than one, \
                 which cannot be stored as one line"
            )));
        }
    }

    let item = element.at(&["Item"]);
    let tax = item.and_then(|i| i.at(&["ClassifiedTaxCategory"]));
    category(position, tax.map_or("", |t| t.text_at(&["ID"])))?;

    Ok(InboundLine {
        description: describe(
            item.map_or("", |i| i.text_at(&["Name"])),
            item.map_or("", |i| i.text_at(&["Description"])),
        ),
        unit: unit_label(billed.attr("unitCode")),
        qty_milli: quantity("BT-129", &billed.text)?,
        unit_price_cents: amount("BT-146", price.text_at(&["PriceAmount"]))?,
        vat_rate_bp: rate_bp("BT-152", tax.map_or("0", |t| t.text_at(&["Percent"])))?,
        net_cents: amount("BT-131", element.text_at(&["LineExtensionAmount"]))?,
    })
}

/// The document's VAT total (BT-110), out of the one or two `cac:TaxTotal`
/// groups it states.
///
/// The second group, when there is one, is BT-111 — the same VAT restated in
/// the currency the supplier keeps books in — and it is told apart by its
/// `currencyID`, never by its position.
fn tax_total<'a>(root: &'a Element, currency: &str) -> &'a str {
    let mut amounts = root
        .children_named("TaxTotal")
        .filter_map(|total| total.child("TaxAmount"));
    let first = amounts.next();
    let matching = first
        .into_iter()
        .chain(amounts)
        .find(|total| total.attr("currencyID").eq_ignore_ascii_case(currency));
    matching.or(first).map_or("", |total| total.text.as_str())
}

/// An optional amount: absent is zero, present is read strictly.
fn optional_amount(term: &str, raw: &str) -> Result<i64> {
    if raw.trim().is_empty() {
        return Ok(0);
    }
    amount(term, raw)
}

/// An optional date: absent is `None`, present is read strictly.
fn optional_date(term: &str, raw: &str) -> Result<Option<time::Date>> {
    if raw.trim().is_empty() {
        return Ok(None);
    }
    date(term, raw).map(Some)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::billing_xml_tree;

    fn tree(xml: &str) -> Element {
        billing_xml_tree::parse(xml).unwrap_or_else(|e| panic!("not XML: {e}"))
    }

    fn refused<T: std::fmt::Debug>(result: Result<T>) -> String {
        match result {
            Err(StoreError::Validation(message)) => message,
            other => panic!("expected a Validation refusal, got {other:?}"),
        }
    }

    #[test]
    fn a_type_code_that_contradicts_the_root_element_is_refused() {
        assert!(check_type_code("380", false).is_ok());
        assert!(check_type_code("381", true).is_ok());
        assert!(
            check_type_code("", true).is_ok(),
            "the root already says so"
        );
        let message = refused(check_type_code("380", true));
        assert!(message.contains("credit note"), "{message}");
        assert!(refused(check_type_code("384", false)).contains("384"));
    }

    #[test]
    fn a_party_prefers_its_registered_name_and_its_vat_scheme() {
        let element = tree(
            r#"<Party>
                 <EndpointID schemeID="EM">rechnung@lieferant.test</EndpointID>
                 <PartyName><Name>Lieferant</Name></PartyName>
                 <PostalAddress>
                   <StreetName>Hauptstraße 5</StreetName>
                   <AdditionalStreetName>Gebäude C</AdditionalStreetName>
                   <CityName>Berlin</CityName>
                   <PostalZone>10115</PostalZone>
                   <Country><IdentificationCode>de</IdentificationCode></Country>
                 </PostalAddress>
                 <PartyTaxScheme><CompanyID>12/345/67890</CompanyID><TaxScheme><ID>FC</ID></TaxScheme></PartyTaxScheme>
                 <PartyTaxScheme><CompanyID>DE811907980</CompanyID><TaxScheme><ID>VAT</ID></TaxScheme></PartyTaxScheme>
                 <PartyLegalEntity><RegistrationName>Lieferant GmbH</RegistrationName><CompanyID>HRB 1234</CompanyID></PartyLegalEntity>
               </Party>"#,
        );
        let party = party(&element);
        assert_eq!(party.name, "Lieferant GmbH", "registered, not trading");
        assert_eq!(party.vat_id, "DE811907980");
        assert_eq!(party.legal_id, "HRB 1234");
        assert_eq!(party.line1, "Hauptstraße 5");
        assert_eq!(party.line2, "Gebäude C");
        assert_eq!(party.postal_code, "10115");
        assert_eq!(party.city, "Berlin");
        assert_eq!(party.country, "DE");
        assert_eq!(party.email, "rechnung@lieferant.test");
    }

    #[test]
    fn a_party_with_only_a_trading_name_and_a_contact_email_still_reads() {
        let element = tree(
            r#"<Party>
                 <EndpointID schemeID="0088">4012345000009</EndpointID>
                 <PartyName><Name>Sole Trader</Name></PartyName>
                 <Contact><ElectronicMail>post@sole.test</ElectronicMail></Contact>
               </Party>"#,
        );
        let party = party(&element);
        assert_eq!(party.name, "Sole Trader");
        // A GLN endpoint is not an email address, so the contact's is used
        // rather than a party identifier pretending to be one.
        assert_eq!(party.email, "post@sole.test");
        assert_eq!(party.vat_id, "");
    }

    #[test]
    fn an_invoice_line_and_a_credit_note_line_read_the_same_way() {
        let invoice_line = tree(
            r#"<InvoiceLine>
                 <ID>1</ID>
                 <InvoicedQuantity unitCode="HUR">15</InvoicedQuantity>
                 <LineExtensionAmount currencyID="EUR">1875.00</LineExtensionAmount>
                 <Item><Name>Consulting</Name>
                   <ClassifiedTaxCategory><ID>S</ID><Percent>21.00</Percent></ClassifiedTaxCategory>
                 </Item>
                 <Price><PriceAmount currencyID="EUR">125.00</PriceAmount></Price>
               </InvoiceLine>"#,
        );
        let read = line(1, &invoice_line, false).unwrap_or_else(|e| panic!("rejected: {e}"));
        assert_eq!(read.description, "Consulting");
        assert_eq!(read.unit, "hour");
        assert_eq!(read.qty_milli, 15_000);
        assert_eq!(read.unit_price_cents, 12_500);
        assert_eq!(read.vat_rate_bp, 2100);
        assert_eq!(read.net_cents, 187_500);

        let credit_line = tree(
            r#"<CreditNoteLine>
                 <ID>1</ID>
                 <CreditedQuantity unitCode="HUR">15</CreditedQuantity>
                 <LineExtensionAmount currencyID="EUR">1875.00</LineExtensionAmount>
                 <Item><Name>Consulting</Name>
                   <ClassifiedTaxCategory><ID>S</ID><Percent>21.00</Percent></ClassifiedTaxCategory>
                 </Item>
                 <Price><PriceAmount currencyID="EUR">125.00</PriceAmount></Price>
               </CreditNoteLine>"#,
        );
        assert_eq!(line(1, &credit_line, true).ok(), Some(read));
        // …and the quantity element of the other kind is not read by mistake.
        assert!(refused(line(1, &credit_line, false)).contains("BT-129"));
    }

    #[test]
    fn the_vat_total_is_the_one_in_the_documents_own_currency() {
        let root = tree(
            r#"<Invoice>
                 <TaxTotal><TaxAmount currencyID="USD">252.00</TaxAmount></TaxTotal>
                 <TaxTotal><TaxAmount currencyID="EUR">216.75</TaxAmount></TaxTotal>
               </Invoice>"#,
        );
        assert_eq!(tax_total(&root, "USD"), "252.00");
        assert_eq!(
            tax_total(&root, "EUR"),
            "216.75",
            "by currency, not position"
        );
        // One group and no currency stated: that figure is the VAT total.
        let plain = tree("<Invoice><TaxTotal><TaxAmount>414.92</TaxAmount></TaxTotal></Invoice>");
        assert_eq!(tax_total(&plain, "EUR"), "414.92");
        assert_eq!(tax_total(&tree("<Invoice/>"), "EUR"), "");
    }

    #[test]
    fn a_document_without_totals_says_so_rather_than_reading_as_worth_nothing() {
        let message = refused(read(&tree("<Invoice><ID>1</ID></Invoice>"), false));
        assert!(message.contains("LegalMonetaryTotal"), "{message}");
    }
}
