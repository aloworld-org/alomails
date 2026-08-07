//! **XRechnung**: the EN 16931 invoice written as OASIS UBL 2.1 XML (alo
//! Billing, wave B1.23).
//!
//! The second of the two syntaxes European law recognises for the same semantic
//! invoice ([`crate::billing_einvoice`]); CII, which Factur-X carries
//! ([`crate::billing_cii`]), is the other. Where Factur-X is a *hybrid* — the
//! XML riding inside the PDF a person reads — XRechnung is the plain file: it
//! is what German public administration must be invoiced with, and what a
//! Peppol access point moves.
//!
//! ## The same invoice, spelled entirely differently
//!
//! Reading this next to [`crate::billing_cii`] is the fastest way to see why
//! [`crate::billing_einvoice`] exists. The two syntaxes carry the identical
//! business terms and disagree about all of:
//!
//! | | CII (Factur-X) | UBL (XRechnung) |
//! |---|---|---|
//! | root | `rsm:CrossIndustryInvoice`, whatever the document is | `ubl:Invoice` **or** `ubl:CreditNote` — a different schema per direction |
//! | dates | `format="102"`, `20260807` | ISO 8601, `2026-08-07` |
//! | currency | on exactly one element; stating it twice is an error | on **every** amount, and omitting it is an error |
//! | lines | `ram:IncludedSupplyChainTradeLineItem`, first | `cac:InvoiceLine` / `cac:CreditNoteLine`, last |
//! | quantity | `ram:BilledQuantity` | `cbc:InvoicedQuantity` / `cbc:CreditedQuantity` |
//!
//! Both are schemas of **sequences**: `cbc:CityName` before `cbc:PostalZone`
//! before `cac:Country` is not a style, it is the difference between a document
//! that validates at a customer's gateway and one that is refused there. The
//! golden files in `tests/golden/` pin this module's order byte for byte.
//!
//! ## Why a credit note is a different document here
//!
//! CII says "credit note" in a type code and changes nothing else. UBL has two
//! root schemas, and a 381 in an `Invoice` element is not a valid document. So
//! [`render`] switches the root, the type-code element and the quantity element
//! together, and [`is_credit_note`] is the single place that decision is made.
//!
//! ## What XRechnung adds on top of EN 16931
//!
//! XRechnung is a CIUS — a narrowing — and it narrows by *requiring* terms the
//! European standard leaves optional: the seller's contact desk, telephone and
//! email, both parties' post codes and cities, and the buyer's reference (the
//! *Leitweg-ID* a German authority is addressed by). Those are
//! [`crate::billing_xrechnung_rules`], checked before this module is ever
//! called; a document that cannot satisfy them is refused with the rule
//! identifiers rather than rendered into a file that will be rejected later.

use time::Date;

use crate::billing_einvoice::{
    CreditTransfer, EInvoice, EInvoiceLine, Party, TypeCode, VatBreakdown,
};
use crate::billing_print::{PrintDocument, Strings};
use crate::billing_xml::{Xml, amount, percent, quantity};

/// BT-24 for XRechnung 3.0: EN 16931 as narrowed by the German CIUS.
///
/// The `#compliant#` form is the standard's own way of saying "this document
/// follows EN 16931 *and* the specification named after it" — a receiving
/// system that only knows the core standard still reads it.
pub const CUSTOMIZATION_ID: &str =
    "urn:cen.eu:en16931:2017#compliant#urn:xoev-de:kosit:standard:xrechnung_3.0";

/// BT-23: the business process the document belongs to.
///
/// The Peppol billing process, which XRechnung adopts — the value a German
/// authority's gateway and a Peppol access point both expect.
pub const PROFILE_ID: &str = "urn:fdc:peppol.eu:2017:poacc:billing:01:1.0";

/// The UBL 2.1 namespaces, minus the root schema's own.
const NS_COMMON: &str = "xmlns:cac=\"urn:oasis:names:specification:ubl:schema:xsd:CommonAggregateComponents-2\" \
     xmlns:cbc=\"urn:oasis:names:specification:ubl:schema:xsd:CommonBasicComponents-2\"";

/// Whether the document is written in the credit-note schema.
fn is_credit_note(invoice: &EInvoice) -> bool {
    invoice.type_code == TypeCode::CreditNote
}

/// The document as XRechnung (UBL 2.1) XML.
///
/// The invoice is expected to have passed both rule checkers first
/// ([`crate::billing_einvoice_rules`] and [`crate::billing_xrechnung_rules`]);
/// this renders what it is given, and a document missing a mandatory term
/// renders without that element rather than inventing one. The route validates
/// before it renders, so an invalid document never reaches a customer.
#[must_use]
pub fn render(invoice: &EInvoice) -> String {
    let credit = is_credit_note(invoice);
    let (root, schema) = if credit {
        (
            "ubl:CreditNote",
            "xmlns:ubl=\"urn:oasis:names:specification:ubl:schema:xsd:CreditNote-2\"",
        )
    } else {
        (
            "ubl:Invoice",
            "xmlns:ubl=\"urn:oasis:names:specification:ubl:schema:xsd:Invoice-2\"",
        )
    };

    let mut xml = Xml::new();
    xml.raw("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n");
    xml.open_with(root, &format!("{schema} {NS_COMMON}"));

    header(&mut xml, invoice, credit);
    party(&mut xml, "cac:AccountingSupplierParty", &invoice.seller);
    party(&mut xml, "cac:AccountingCustomerParty", &invoice.buyer);
    payment_means(&mut xml, invoice);
    if !invoice.payment_terms.trim().is_empty() {
        xml.open("cac:PaymentTerms");
        xml.leaf("cbc:Note", invoice.payment_terms.trim());
        xml.close("cac:PaymentTerms");
    }
    tax_total(&mut xml, invoice);
    monetary_total(&mut xml, invoice);
    for line in &invoice.lines {
        trade_line(&mut xml, invoice, line, credit);
    }

    xml.close(root);
    xml.finish()
}

/// BT-24, BT-23, BT-1, BT-2, BT-9, BT-3, BT-22, BT-5, BT-6, BT-10, BT-25:
/// everything the document says about itself, in the schema's order.
fn header(xml: &mut Xml, invoice: &EInvoice, credit: bool) {
    xml.leaf("cbc:CustomizationID", CUSTOMIZATION_ID);
    xml.leaf("cbc:ProfileID", PROFILE_ID);
    xml.leaf("cbc:ID", &invoice.number);
    if let Some(issued) = invoice.issue_date {
        xml.leaf("cbc:IssueDate", &date_iso(issued));
    }
    // BT-9 on an invoice only. A credit note is not payable, so a due date on
    // one would name a deadline for a payment nobody owes — and UBL puts the
    // credit note's own settlement date somewhere else entirely.
    if !credit && let Some(due) = invoice.due_date {
        xml.leaf("cbc:DueDate", &date_iso(due));
    }
    xml.leaf(
        if credit {
            "cbc:CreditNoteTypeCode"
        } else {
            "cbc:InvoiceTypeCode"
        },
        invoice.type_code.as_code(),
    );
    if !invoice.note.trim().is_empty() {
        xml.leaf("cbc:Note", invoice.note.trim());
    }
    xml.leaf("cbc:DocumentCurrencyCode", &invoice.currency);
    if let Some(tax_currency) = &invoice.tax_currency {
        xml.leaf("cbc:TaxCurrencyCode", &tax_currency.code);
    }
    if !invoice.buyer_reference.trim().is_empty() {
        xml.leaf("cbc:BuyerReference", invoice.buyer_reference.trim());
    }
    // BT-25: the invoice this document corrects. UBL states it as a reference
    // to a whole document, which is why it is a group and not a field.
    if !invoice.preceding_invoice.trim().is_empty() {
        xml.open("cac:BillingReference");
        xml.open("cac:InvoiceDocumentReference");
        xml.leaf("cbc:ID", invoice.preceding_invoice.trim());
        xml.close("cac:InvoiceDocumentReference");
        xml.close("cac:BillingReference");
    }
}

/// One party (BG-4 seller, BG-7 buyer), in the schema's own order.
///
/// Three places carry a name in UBL and they are not interchangeable: the
/// **legal entity's** registration name is BT-27/BT-44, the one the standard
/// requires; `cac:PartyName` is a *trading* name we do not hold separately; and
/// the contact's name is a desk, not the company. Only the first two of those
/// are written.
fn party(xml: &mut Xml, tag: &str, party: &Party) {
    xml.open(tag);
    xml.open("cac:Party");
    if !party.email.trim().is_empty() {
        // BT-34/BT-49, with the EAS scheme `EM` for an email address: an
        // address a receiving system may deliver to, which is not the same
        // claim as "somebody reads this mailbox".
        xml.leaf_with("cbc:EndpointID", "schemeID=\"EM\"", party.email.trim());
    }
    xml.open("cac:PostalAddress");
    if !party.line1.is_empty() {
        xml.leaf("cbc:StreetName", &party.line1);
    }
    if !party.line2.is_empty() {
        xml.leaf("cbc:AdditionalStreetName", &party.line2);
    }
    if !party.city.is_empty() {
        xml.leaf("cbc:CityName", &party.city);
    }
    if !party.postal_code.is_empty() {
        xml.leaf("cbc:PostalZone", &party.postal_code);
    }
    xml.open("cac:Country");
    xml.leaf("cbc:IdentificationCode", &party.country);
    xml.close("cac:Country");
    xml.close("cac:PostalAddress");
    if !party.vat_id.trim().is_empty() {
        xml.open("cac:PartyTaxScheme");
        xml.leaf("cbc:CompanyID", party.vat_id.trim());
        xml.open("cac:TaxScheme");
        xml.leaf("cbc:ID", "VAT");
        xml.close("cac:TaxScheme");
        xml.close("cac:PartyTaxScheme");
    }
    xml.open("cac:PartyLegalEntity");
    xml.leaf("cbc:RegistrationName", &party.name);
    if !party.legal_id.trim().is_empty() {
        xml.leaf("cbc:CompanyID", party.legal_id.trim());
    }
    xml.close("cac:PartyLegalEntity");
    // BG-6 / BG-8: written only when the party names a contact point, which
    // the seller does and the buyer does not. A group holding nothing but the
    // customer's email would claim BT-58 — "the contact person's address" —
    // about the one address we hold, which is the delivery address in
    // `cbc:EndpointID` above and not the same statement.
    if !party.contact_name.trim().is_empty() {
        xml.open("cac:Contact");
        if !party.contact_name.trim().is_empty() {
            xml.leaf("cbc:Name", party.contact_name.trim());
        }
        if !party.phone.trim().is_empty() {
            xml.leaf("cbc:Telephone", party.phone.trim());
        }
        if !party.email.trim().is_empty() {
            xml.leaf("cbc:ElectronicMail", party.email.trim());
        }
        xml.close("cac:Contact");
    }
    xml.close("cac:Party");
    xml.close(tag);
}

/// BG-16: how the money moves.
///
/// XRechnung requires this group on every document (BR-DE-1), including one
/// nothing is payable on. A credit note therefore states UNTDID 4461 code `1`,
/// **"instrument not defined"** — which is the truth: the refund is arranged
/// between the parties and the document does not name an account. Writing the
/// seller's own IBAN there instead would invite a customer to pay a document
/// that owes *them*. An invoice from a tenant that has stated no bank account
/// says the same thing for the same reason.
fn payment_means(xml: &mut Xml, invoice: &EInvoice) {
    let bank: Option<&CreditTransfer> = match invoice.type_code {
        TypeCode::Invoice => invoice.credit_transfer.as_ref(),
        TypeCode::CreditNote => None,
    };
    xml.open("cac:PaymentMeans");
    match bank {
        // 30: credit transfer — the payer moves the money.
        Some(_) => xml.leaf("cbc:PaymentMeansCode", "30"),
        None => xml.leaf("cbc:PaymentMeansCode", "1"),
    }
    // BT-83: what the payer quotes so the money can be matched to the document.
    if invoice.type_code == TypeCode::Invoice && !invoice.number.is_empty() {
        xml.leaf("cbc:PaymentID", &invoice.number);
    }
    if let Some(bank) = bank {
        xml.open("cac:PayeeFinancialAccount");
        xml.leaf("cbc:ID", &bank.iban);
        if !bank.holder.trim().is_empty() {
            xml.leaf("cbc:Name", bank.holder.trim());
        }
        if !bank.bic.trim().is_empty() {
            xml.open("cac:FinancialInstitutionBranch");
            xml.leaf("cbc:ID", bank.bic.trim());
            xml.close("cac:FinancialInstitutionBranch");
        }
        xml.close("cac:PayeeFinancialAccount");
    }
    xml.close("cac:PaymentMeans");
}

/// BG-23 and BT-110/BT-111: the VAT, broken down and totalled.
///
/// A document raised in another currency states its VAT **twice**, and UBL
/// spells that as a *second* `cac:TaxTotal` carrying nothing but the amount in
/// the accounting currency — the subtotals belong to the document currency and
/// are not repeated.
fn tax_total(xml: &mut Xml, invoice: &EInvoice) {
    xml.open("cac:TaxTotal");
    money(
        xml,
        "cbc:TaxAmount",
        invoice.tax_total_cents,
        &invoice.currency,
    );
    for group in &invoice.vat_breakdown {
        tax_subtotal(xml, group, &invoice.currency);
    }
    xml.close("cac:TaxTotal");
    if let Some(tax_currency) = &invoice.tax_currency {
        xml.open("cac:TaxTotal");
        money(
            xml,
            "cbc:TaxAmount",
            tax_currency.tax_cents,
            &tax_currency.code,
        );
        xml.close("cac:TaxTotal");
    }
}

/// One VAT breakdown group (BG-23).
fn tax_subtotal(xml: &mut Xml, group: &VatBreakdown, currency: &str) {
    xml.open("cac:TaxSubtotal");
    money(xml, "cbc:TaxableAmount", group.taxable_cents, currency);
    money(xml, "cbc:TaxAmount", group.tax_cents, currency);
    xml.open("cac:TaxCategory");
    xml.leaf("cbc:ID", group.category.as_code());
    xml.leaf("cbc:Percent", &percent(group.rate_bp));
    xml.open("cac:TaxScheme");
    xml.leaf("cbc:ID", "VAT");
    xml.close("cac:TaxScheme");
    xml.close("cac:TaxCategory");
    xml.close("cac:TaxSubtotal");
}

/// BG-22: the document totals.
fn monetary_total(xml: &mut Xml, invoice: &EInvoice) {
    let currency = &invoice.currency;
    xml.open("cac:LegalMonetaryTotal");
    money(
        xml,
        "cbc:LineExtensionAmount",
        invoice.line_total_cents,
        currency,
    );
    money(
        xml,
        "cbc:TaxExclusiveAmount",
        invoice.tax_basis_cents,
        currency,
    );
    money(
        xml,
        "cbc:TaxInclusiveAmount",
        invoice.grand_total_cents,
        currency,
    );
    money(
        xml,
        "cbc:PayableAmount",
        invoice.due_payable_cents,
        currency,
    );
    xml.close("cac:LegalMonetaryTotal");
}

/// BG-25: one line, under the element name its document's schema uses.
fn trade_line(xml: &mut Xml, invoice: &EInvoice, line: &EInvoiceLine, credit: bool) {
    let (tag, quantity_tag) = if credit {
        ("cac:CreditNoteLine", "cbc:CreditedQuantity")
    } else {
        ("cac:InvoiceLine", "cbc:InvoicedQuantity")
    };
    let currency = &invoice.currency;

    xml.open(tag);
    xml.leaf("cbc:ID", &line.id);
    xml.leaf_with(
        quantity_tag,
        &format!("unitCode=\"{}\"", line.unit_code),
        &quantity(line.qty_milli),
    );
    money(xml, "cbc:LineExtensionAmount", line.net_cents, currency);

    xml.open("cac:Item");
    // BT-154 before BT-153: what the item is, then what it is called, because
    // that is the order the UBL sequence is defined in.
    if !line.description.is_empty() {
        xml.leaf("cbc:Description", &line.description);
    }
    xml.leaf("cbc:Name", &line.name);
    xml.open("cac:ClassifiedTaxCategory");
    xml.leaf("cbc:ID", line.category.as_code());
    xml.leaf("cbc:Percent", &percent(line.rate_bp));
    xml.open("cac:TaxScheme");
    xml.leaf("cbc:ID", "VAT");
    xml.close("cac:TaxScheme");
    xml.close("cac:ClassifiedTaxCategory");
    xml.close("cac:Item");

    xml.open("cac:Price");
    money(xml, "cbc:PriceAmount", line.unit_price_cents, currency);
    xml.close("cac:Price");

    xml.close(tag);
}

// ---- the download ------------------------------------------------------------

/// The name the XML is saved under: [`crate::billing_xml::file_name`] with this
/// syntax's suffix.
#[must_use]
pub fn file_name(doc: &PrintDocument<'_>, s: &Strings) -> String {
    crate::billing_xml::file_name(doc, s, "xrechnung")
}

// ---- formatting --------------------------------------------------------------

/// An amount with the currency stated on it, which UBL requires of every one.
fn money(xml: &mut Xml, tag: &str, cents: i64, currency: &str) {
    xml.leaf_with(
        tag,
        &format!("currencyID=\"{}\"", crate::billing_xml::esc(currency)),
        &amount(cents),
    );
}

/// A date as ISO 8601: `YYYY-MM-DD`. UBL's dates are typed, not coded, so
/// there is no format attribute beside them.
fn date_iso(value: Date) -> String {
    format!(
        "{:04}-{:02}-{:02}",
        value.year(),
        u8::from(value.month()),
        value.day()
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::billing_einvoice::{SPECIFICATION_ID, TaxCurrency, sample};

    /// Walks the tags and returns the element names left open, so a test can
    /// assert the document balances rather than that it looks balanced.
    fn unclosed(xml: &str) -> Vec<String> {
        let mut stack: Vec<String> = Vec::new();
        for tag in xml.split('<').skip(1) {
            let body = tag.split('>').next().unwrap_or_default();
            if body.starts_with('?') || body.starts_with('!') {
                continue;
            }
            let name = body
                .trim_start_matches('/')
                .split([' ', '/'])
                .next()
                .unwrap_or_default()
                .to_owned();
            if body.starts_with('/') {
                assert_eq!(
                    stack.pop().as_deref(),
                    Some(name.as_str()),
                    "closing {name}"
                );
            } else if !body.ends_with('/') {
                stack.push(name);
            }
        }
        stack
    }

    /// The order in which the given markers appear, for asserting a sequence.
    fn positions(xml: &str, markers: &[&str]) -> Vec<usize> {
        markers
            .iter()
            .map(|marker| {
                xml.find(marker)
                    .unwrap_or_else(|| panic!("{marker} is not in the document"))
            })
            .collect()
    }

    #[test]
    fn the_customization_still_promises_the_core_standard() {
        // The `#compliant#` form is a promise about the core standard, and the
        // two identifiers must not be able to drift apart.
        assert!(
            CUSTOMIZATION_ID.starts_with(SPECIFICATION_ID),
            "{CUSTOMIZATION_ID}"
        );
    }

    #[test]
    fn an_invoice_is_an_invoice_document_in_the_invoice_schema() {
        let xml = render(&sample());
        assert!(xml.starts_with("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<ubl:Invoice "));
        assert!(xml.contains("schema:xsd:Invoice-2"));
        assert!(xml.contains("<cbc:InvoiceTypeCode>380</cbc:InvoiceTypeCode>"));
        assert!(xml.contains("<cac:InvoiceLine>"));
        assert!(xml.contains("<cbc:InvoicedQuantity unitCode=\"HUR\">1.5</cbc:InvoicedQuantity>"));
        assert!(xml.ends_with("</ubl:Invoice>\n"));
        assert_eq!(unclosed(&xml), Vec::<String>::new());
    }

    #[test]
    fn a_credit_note_is_a_different_schema_and_not_merely_a_different_code() {
        let mut credit = sample();
        credit.type_code = TypeCode::CreditNote;
        credit.preceding_invoice = "INV-2026-00001".to_owned();
        credit.payment_terms = String::new();
        let xml = render(&credit);
        assert!(xml.starts_with("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<ubl:CreditNote "));
        assert!(xml.contains("schema:xsd:CreditNote-2"));
        assert!(xml.contains("<cbc:CreditNoteTypeCode>381</cbc:CreditNoteTypeCode>"));
        assert!(xml.contains("<cac:CreditNoteLine>"));
        assert!(xml.contains("<cbc:CreditedQuantity unitCode=\"HUR\">1.5</cbc:CreditedQuantity>"));
        assert!(!xml.contains("InvoiceTypeCode"));
        assert!(!xml.contains("<cac:InvoiceLine>"));
        // It names what it corrects, states no due date, and asks for nothing:
        // no account, no payment reference, and the instrument undefined.
        assert!(xml.contains("<cac:InvoiceDocumentReference>"));
        assert!(xml.contains("<cbc:ID>INV-2026-00001</cbc:ID>"));
        assert!(!xml.contains("DueDate"));
        assert!(!xml.contains("PayeeFinancialAccount"));
        assert!(!xml.contains("PaymentID"));
        assert!(xml.contains("<cbc:PaymentMeansCode>1</cbc:PaymentMeansCode>"));
        assert_eq!(unclosed(&xml), Vec::<String>::new());
    }

    #[test]
    fn every_amount_states_its_currency() {
        // The opposite of CII, where stating it twice is the error.
        let xml = render(&sample());
        for tag in [
            "cbc:TaxAmount",
            "cbc:TaxableAmount",
            "cbc:LineExtensionAmount",
            "cbc:TaxExclusiveAmount",
            "cbc:TaxInclusiveAmount",
            "cbc:PayableAmount",
            "cbc:PriceAmount",
        ] {
            assert!(
                xml.contains(&format!("<{tag} currencyID=\"EUR\">")),
                "{tag} carries no currency"
            );
            assert!(
                !xml.contains(&format!("<{tag}>")),
                "{tag} appears without a currency"
            );
        }
        assert!(xml.contains("<cbc:PayableAmount currencyID=\"EUR\">226.88</cbc:PayableAmount>"));
    }

    #[test]
    fn a_foreign_currency_document_states_its_vat_in_a_second_tax_total() {
        let mut invoice = sample();
        invoice.currency = "USD".to_owned();
        invoice.tax_currency = Some(TaxCurrency {
            code: "EUR".to_owned(),
            tax_cents: 3_387,
        });
        let xml = render(&invoice);
        assert_eq!(xml.matches("<cac:TaxTotal>").count(), 2);
        assert!(xml.contains("<cbc:TaxCurrencyCode>EUR</cbc:TaxCurrencyCode>"));
        assert!(xml.contains("<cbc:TaxAmount currencyID=\"USD\">39.38</cbc:TaxAmount>"));
        assert!(xml.contains("<cbc:TaxAmount currencyID=\"EUR\">33.87</cbc:TaxAmount>"));
        // The second one carries the amount and nothing else: the breakdown
        // belongs to the currency the document was raised in.
        assert_eq!(xml.matches("<cac:TaxSubtotal>").count(), 1);
    }

    #[test]
    fn the_document_is_written_in_the_order_the_schema_sequences_it() {
        let mut invoice = sample();
        invoice.preceding_invoice = "INV-2026-00000".to_owned();
        invoice.tax_currency = Some(TaxCurrency {
            code: "EUR".to_owned(),
            tax_cents: 3_938,
        });
        let xml = render(&invoice);
        let order = positions(
            &xml,
            &[
                "<cbc:CustomizationID>",
                "<cbc:ProfileID>",
                "<cbc:ID>",
                "<cbc:IssueDate>",
                "<cbc:DueDate>",
                "<cbc:InvoiceTypeCode>",
                "<cbc:Note>",
                "<cbc:DocumentCurrencyCode>",
                "<cbc:TaxCurrencyCode>",
                "<cbc:BuyerReference>",
                "<cac:BillingReference>",
                "<cac:AccountingSupplierParty>",
                "<cac:AccountingCustomerParty>",
                "<cac:PaymentMeans>",
                "<cac:PaymentTerms>",
                "<cac:TaxTotal>",
                "<cac:LegalMonetaryTotal>",
                "<cac:InvoiceLine>",
            ],
        );
        assert!(
            order.windows(2).all(|pair| pair[0] < pair[1]),
            "the document is out of sequence: {order:?}"
        );
        // …and inside a party, where a reordering is just as fatal.
        let party = positions(
            &xml,
            &[
                "<cbc:EndpointID",
                "<cac:PostalAddress>",
                "<cbc:StreetName>",
                "<cbc:CityName>",
                "<cbc:PostalZone>",
                "<cac:Country>",
                "<cac:PartyTaxScheme>",
                "<cac:PartyLegalEntity>",
                "<cac:Contact>",
            ],
        );
        assert!(
            party.windows(2).all(|pair| pair[0] < pair[1]),
            "the seller is out of sequence: {party:?}"
        );
    }

    #[test]
    fn the_seller_states_a_contact_desk_and_the_buyer_states_none() {
        let xml = render(&sample());
        assert_eq!(xml.matches("<cac:Contact>").count(), 1);
        assert!(xml.contains("<cbc:Telephone>+31 20 123 4567</cbc:Telephone>"));
        assert!(xml.contains("<cbc:ElectronicMail>billing@alo.test</cbc:ElectronicMail>"));
        // The buyer's email is its electronic address, not a contact person.
        assert!(
            xml.contains("<cbc:EndpointID schemeID=\"EM\">einkauf@kunde.test</cbc:EndpointID>")
        );
    }

    #[test]
    fn an_invoice_without_a_bank_account_says_the_instrument_is_undefined() {
        // BR-DE-1 wants the group on every document; what it must not do is
        // invent an account nobody stated.
        let mut invoice = sample();
        invoice.credit_transfer = None;
        let xml = render(&invoice);
        assert!(xml.contains("<cbc:PaymentMeansCode>1</cbc:PaymentMeansCode>"));
        assert!(!xml.contains("PayeeFinancialAccount"));
        // The payment reference survives: it is how the money is matched.
        assert!(xml.contains("<cbc:PaymentID>INV-2026-00001</cbc:PaymentID>"));
    }

    #[test]
    fn every_string_in_the_document_is_escaped() {
        let mut invoice = sample();
        invoice.buyer.name = "Meier & Söhne <GmbH>".to_owned();
        invoice.note = "He said \"pay it\" — 'soon'".to_owned();
        invoice.lines[0].name = "Bolt <M8>".to_owned();
        let xml = render(&invoice);
        assert!(xml.contains(
            "<cbc:RegistrationName>Meier &amp; S\u{f6}hne &lt;GmbH&gt;</cbc:RegistrationName>"
        ));
        assert!(xml.contains("&quot;pay it&quot; \u{2014} &apos;soon&apos;"));
        assert!(xml.contains("<cbc:Name>Bolt &lt;M8&gt;</cbc:Name>"));
        assert_eq!(unclosed(&xml), Vec::<String>::new());
    }

    #[test]
    fn a_document_with_nothing_optional_on_it_still_closes_everything() {
        let mut bare = sample();
        bare.note = String::new();
        bare.buyer_reference = String::new();
        bare.payment_terms = String::new();
        bare.due_date = None;
        bare.credit_transfer = None;
        bare.seller.legal_id = String::new();
        bare.seller.email = String::new();
        bare.seller.phone = String::new();
        bare.seller.contact_name = String::new();
        bare.buyer.email = String::new();
        bare.buyer.vat_id = String::new();
        bare.buyer.line2 = String::new();
        let xml = render(&bare);
        assert_eq!(unclosed(&xml), Vec::<String>::new());
        // No empty groups where a term is simply not stated.
        assert!(!xml.contains("<cac:Contact>"));
        // Only the seller states a VAT registration: the buyer's was cleared.
        assert_eq!(xml.matches("<cac:PartyTaxScheme>").count(), 1);
        assert!(!xml.contains("<cbc:Note>"));
    }
}
