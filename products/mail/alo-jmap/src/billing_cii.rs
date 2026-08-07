//! **Factur-X**: the EN 16931 invoice written as UN/CEFACT Cross Industry
//! Invoice XML (alo Billing, wave B1.22).
//!
//! One of the two syntaxes European law recognises for the same semantic
//! invoice ([`crate::billing_einvoice`]); UBL, which XRechnung uses, is the
//! other and lands at B1.23. Factur-X (identical to ZUGFeRD 2.x, and the
//! French/German mandate's carrier) is the *hybrid* form: the XML travels
//! **inside** the PDF the human reads, so one file satisfies both readers and
//! neither can be sent without the other.
//!
//! ## What is written, and in what order
//!
//! CII is a schema of **sequences**, not a bag of elements: `ram:PostcodeCode`
//! before `ram:LineOne` before `ram:CityName` before `ram:CountryID` is not a
//! style, it is the difference between a document that validates and one that
//! does not. So this module is ordered the way the schema is, and the golden
//! files in `tests/golden/` pin that order byte for byte — a reordering is a
//! failing test, not a surprise at a customer's gateway.
//!
//! Three conventions worth knowing when reading it:
//!
//! - **`currencyID` appears on exactly one element**, `ram:TaxTotalAmount`,
//!   where the profile requires it. Every other amount inherits
//!   `ram:InvoiceCurrencyCode`, and stating it again is a validation error in
//!   the EN 16931 profile rather than helpful redundancy.
//! - **Dates are `format="102"`**, `YYYYMMDD` — the UNTDID 2379 code for a
//!   calendar date without a time. An invoice date has no clock on it.
//! - **Every value goes through [`esc`]**, including the ones that "cannot"
//!   contain a `&`: the seller's own bank name and the customer's typed
//!   description are the same kind of string here.
//!
//! ## Not yet PDF/A-3
//!
//! Factur-X asks for a PDF/A-3 carrier. What the writer produces
//! ([`alo_pdf::attachment`]) is everything else the hybrid needs — the
//! attachment, its `/AFRelationship /Alternative`, the `/AF` array, the
//! embedded-files name tree and this module's XMP packet — but not the
//! embedded font and output-intent profile PDF/A additionally requires, which
//! are licensed binaries a human chooses (`docs/design/billing.md`). The XMP
//! written here therefore describes **the attached XML** and never claims a
//! PDF/A conformance level the file does not have.

use crate::billing_einvoice::{EInvoice, SPECIFICATION_ID, TypeCode};
use crate::billing_print::{PrintDocument, Strings};
use crate::billing_xml::{Xml, amount, esc, percent, quantity};

/// The file name Factur-X mandates for the XML inside the PDF.
///
/// Not a preference: a receiving system looks the attachment up by this exact
/// name, so it is a constant and never derived from the document.
pub const ATTACHMENT_NAME: &str = "factur-x.xml";

/// The MIME type of the attachment.
pub const ATTACHMENT_MIME: &str = "text/xml";

/// The profile the XMP packet declares for the attached XML.
const CONFORMANCE_LEVEL: &str = "EN 16931";

/// The Factur-X version the XMP packet declares.
const FACTURX_VERSION: &str = "1.0";

/// The document as Factur-X CII XML.
///
/// The invoice is expected to have passed
/// [`crate::billing_einvoice_rules::violations`] first — this renders what it
/// is given, and a document missing a mandatory term renders without that
/// element rather than inventing one. The route validates before it renders,
/// so an invalid document never reaches a customer; a test that renders one
/// deliberately gets to see exactly what is missing.
#[must_use]
pub fn render(invoice: &EInvoice) -> String {
    let mut xml = Xml::new();
    xml.raw("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n");
    xml.open_with(
        "rsm:CrossIndustryInvoice",
        "xmlns:rsm=\"urn:un:unece:uncefact:data:standard:CrossIndustryInvoice:100\" \
         xmlns:ram=\"urn:un:unece:uncefact:data:standard:ReusableAggregateBusinessInformationEntity:100\" \
         xmlns:udt=\"urn:un:unece:uncefact:data:standard:UnqualifiedDataType:100\"",
    );

    context(&mut xml);
    document(&mut xml, invoice);

    xml.open("rsm:SupplyChainTradeTransaction");
    for line in &invoice.lines {
        trade_line(&mut xml, line);
    }
    agreement(&mut xml, invoice);
    // Mandatory in the schema and empty in our documents: we invoice for
    // services and goods whose delivery date is the invoice's own, and BT-72
    // (actual delivery date) is optional. An element that is there and says
    // nothing is the schema's shape, not a placeholder.
    xml.empty("ram:ApplicableHeaderTradeDelivery");
    settlement(&mut xml, invoice);
    xml.close("rsm:SupplyChainTradeTransaction");

    xml.close("rsm:CrossIndustryInvoice");
    xml.finish()
}

/// BT-24: which specification the document follows.
fn context(xml: &mut Xml) {
    xml.open("rsm:ExchangedDocumentContext");
    xml.open("ram:GuidelineSpecifiedDocumentContextParameter");
    xml.leaf("ram:ID", SPECIFICATION_ID);
    xml.close("ram:GuidelineSpecifiedDocumentContextParameter");
    xml.close("rsm:ExchangedDocumentContext");
}

/// BT-1, BT-3, BT-2, BT-22: what the document is.
fn document(xml: &mut Xml, invoice: &EInvoice) {
    xml.open("rsm:ExchangedDocument");
    xml.leaf("ram:ID", &invoice.number);
    xml.leaf("ram:TypeCode", invoice.type_code.as_code());
    if let Some(issued) = invoice.issue_date {
        xml.open("ram:IssueDateTime");
        xml.leaf_with("udt:DateTimeString", "format=\"102\"", &date_102(issued));
        xml.close("ram:IssueDateTime");
    }
    if !invoice.note.trim().is_empty() {
        xml.open("ram:IncludedNote");
        xml.leaf("ram:Content", invoice.note.trim());
        xml.close("ram:IncludedNote");
    }
    xml.close("rsm:ExchangedDocument");
}

/// BG-25: one invoice line.
fn trade_line(xml: &mut Xml, line: &crate::billing_einvoice::EInvoiceLine) {
    xml.open("ram:IncludedSupplyChainTradeLineItem");

    xml.open("ram:AssociatedDocumentLineDocument");
    xml.leaf("ram:LineID", &line.id);
    xml.close("ram:AssociatedDocumentLineDocument");

    xml.open("ram:SpecifiedTradeProduct");
    xml.leaf("ram:Name", &line.name);
    if !line.description.is_empty() {
        xml.leaf("ram:Description", &line.description);
    }
    xml.close("ram:SpecifiedTradeProduct");

    xml.open("ram:SpecifiedLineTradeAgreement");
    xml.open("ram:NetPriceProductTradePrice");
    xml.leaf("ram:ChargeAmount", &amount(line.unit_price_cents));
    xml.close("ram:NetPriceProductTradePrice");
    xml.close("ram:SpecifiedLineTradeAgreement");

    xml.open("ram:SpecifiedLineTradeDelivery");
    xml.leaf_with(
        "ram:BilledQuantity",
        &format!("unitCode=\"{}\"", line.unit_code),
        &quantity(line.qty_milli),
    );
    xml.close("ram:SpecifiedLineTradeDelivery");

    xml.open("ram:SpecifiedLineTradeSettlement");
    xml.open("ram:ApplicableTradeTax");
    xml.leaf("ram:TypeCode", "VAT");
    xml.leaf("ram:CategoryCode", line.category.as_code());
    xml.leaf("ram:RateApplicablePercent", &percent(line.rate_bp));
    xml.close("ram:ApplicableTradeTax");
    xml.open("ram:SpecifiedTradeSettlementLineMonetarySummation");
    xml.leaf("ram:LineTotalAmount", &amount(line.net_cents));
    xml.close("ram:SpecifiedTradeSettlementLineMonetarySummation");
    xml.close("ram:SpecifiedLineTradeSettlement");

    xml.close("ram:IncludedSupplyChainTradeLineItem");
}

/// BT-10, BG-4, BG-7: who is trading with whom.
fn agreement(xml: &mut Xml, invoice: &EInvoice) {
    xml.open("ram:ApplicableHeaderTradeAgreement");
    if !invoice.buyer_reference.trim().is_empty() {
        xml.leaf("ram:BuyerReference", invoice.buyer_reference.trim());
    }
    party(xml, "ram:SellerTradeParty", &invoice.seller);
    party(xml, "ram:BuyerTradeParty", &invoice.buyer);
    xml.close("ram:ApplicableHeaderTradeAgreement");
}

/// One party, in the schema's own order.
fn party(xml: &mut Xml, tag: &str, party: &crate::billing_einvoice::Party) {
    xml.open(tag);
    xml.leaf("ram:Name", &party.name);
    if !party.legal_id.trim().is_empty() {
        xml.open("ram:SpecifiedLegalOrganization");
        xml.leaf("ram:ID", party.legal_id.trim());
        xml.close("ram:SpecifiedLegalOrganization");
    }
    xml.open("ram:PostalTradeAddress");
    if !party.postal_code.is_empty() {
        xml.leaf("ram:PostcodeCode", &party.postal_code);
    }
    if !party.line1.is_empty() {
        xml.leaf("ram:LineOne", &party.line1);
    }
    if !party.line2.is_empty() {
        xml.leaf("ram:LineTwo", &party.line2);
    }
    if !party.city.is_empty() {
        xml.leaf("ram:CityName", &party.city);
    }
    xml.leaf("ram:CountryID", &party.country);
    xml.close("ram:PostalTradeAddress");
    if !party.email.trim().is_empty() {
        xml.open("ram:URIUniversalCommunication");
        // `EM` is the electronic-address scheme for an email address (EAS
        // code list): the address a receiving system may deliver to, which is
        // not the same claim as "somebody reads this mailbox".
        xml.leaf_with("ram:URIID", "schemeID=\"EM\"", party.email.trim());
        xml.close("ram:URIUniversalCommunication");
    }
    if !party.vat_id.trim().is_empty() {
        xml.open("ram:SpecifiedTaxRegistration");
        xml.leaf_with("ram:ID", "schemeID=\"VA\"", party.vat_id.trim());
        xml.close("ram:SpecifiedTaxRegistration");
    }
    xml.close(tag);
}

/// BG-16 … BG-23, BT-5/6, BG-22: the money and how it is settled.
fn settlement(xml: &mut Xml, invoice: &EInvoice) {
    xml.open("ram:ApplicableHeaderTradeSettlement");
    // BT-83: what the payer quotes so the money can be matched to the
    // document. The invoice number, which is what the paper asks for too.
    if invoice.type_code == TypeCode::Invoice && !invoice.number.is_empty() {
        xml.leaf("ram:PaymentReference", &invoice.number);
    }
    if let Some(tax_currency) = &invoice.tax_currency {
        xml.leaf("ram:TaxCurrencyCode", &tax_currency.code);
    }
    xml.leaf("ram:InvoiceCurrencyCode", &invoice.currency);

    // A credit note carries no payment instructions: nothing is payable on it,
    // and an IBAN on one invites a customer to pay a document that owes them.
    if let (Some(bank), TypeCode::Invoice) = (&invoice.credit_transfer, invoice.type_code) {
        xml.open("ram:SpecifiedTradeSettlementPaymentMeans");
        // UNTDID 4461 code 30, "credit transfer": the payer moves the money.
        xml.leaf("ram:TypeCode", "30");
        xml.open("ram:PayeePartyCreditorFinancialAccount");
        xml.leaf("ram:IBANID", &bank.iban);
        if !bank.holder.trim().is_empty() {
            xml.leaf("ram:AccountName", bank.holder.trim());
        }
        xml.close("ram:PayeePartyCreditorFinancialAccount");
        if !bank.bic.trim().is_empty() {
            xml.open("ram:PayeeSpecifiedCreditorFinancialInstitution");
            xml.leaf("ram:BICID", bank.bic.trim());
            xml.close("ram:PayeeSpecifiedCreditorFinancialInstitution");
        }
        xml.close("ram:SpecifiedTradeSettlementPaymentMeans");
    }

    for group in &invoice.vat_breakdown {
        xml.open("ram:ApplicableTradeTax");
        xml.leaf("ram:CalculatedAmount", &amount(group.tax_cents));
        xml.leaf("ram:TypeCode", "VAT");
        xml.leaf("ram:BasisAmount", &amount(group.taxable_cents));
        xml.leaf("ram:CategoryCode", group.category.as_code());
        xml.leaf("ram:RateApplicablePercent", &percent(group.rate_bp));
        xml.close("ram:ApplicableTradeTax");
    }

    if !invoice.payment_terms.trim().is_empty() || invoice.due_date.is_some() {
        xml.open("ram:SpecifiedTradePaymentTerms");
        if !invoice.payment_terms.trim().is_empty() {
            xml.leaf("ram:Description", invoice.payment_terms.trim());
        }
        if let Some(due) = invoice.due_date {
            xml.open("ram:DueDateDateTime");
            xml.leaf_with("udt:DateTimeString", "format=\"102\"", &date_102(due));
            xml.close("ram:DueDateDateTime");
        }
        xml.close("ram:SpecifiedTradePaymentTerms");
    }

    xml.open("ram:SpecifiedTradeSettlementHeaderMonetarySummation");
    xml.leaf("ram:LineTotalAmount", &amount(invoice.line_total_cents));
    xml.leaf("ram:TaxBasisTotalAmount", &amount(invoice.tax_basis_cents));
    xml.leaf_with(
        "ram:TaxTotalAmount",
        &format!("currencyID=\"{}\"", esc(&invoice.currency)),
        &amount(invoice.tax_total_cents),
    );
    // BT-111: the same VAT in the currency the issuer keeps books in — a
    // second element of the same name, which is how CII expresses it and why
    // the currency attribute is mandatory on this one element.
    if let Some(tax_currency) = &invoice.tax_currency {
        xml.leaf_with(
            "ram:TaxTotalAmount",
            &format!("currencyID=\"{}\"", esc(&tax_currency.code)),
            &amount(tax_currency.tax_cents),
        );
    }
    xml.leaf("ram:GrandTotalAmount", &amount(invoice.grand_total_cents));
    xml.leaf("ram:DuePayableAmount", &amount(invoice.due_payable_cents));
    xml.close("ram:SpecifiedTradeSettlementHeaderMonetarySummation");

    if !invoice.preceding_invoice.trim().is_empty() {
        xml.open("ram:InvoiceReferencedDocument");
        xml.leaf("ram:IssuerAssignedID", invoice.preceding_invoice.trim());
        xml.close("ram:InvoiceReferencedDocument");
    }

    xml.close("ram:ApplicableHeaderTradeSettlement");
}

/// The XMP packet that tells a reader the PDF carries a Factur-X invoice.
///
/// Two blocks, and only two:
///
/// - the **extension schema**, which is not optional decoration — XMP requires
///   a schema it does not know to be described before it is used, and a
///   Factur-X reader that finds `fx:DocumentFileName` without the description
///   is entitled to ignore it;
/// - the **description itself**: the attachment's name, that it is an invoice,
///   and which profile of EN 16931 it follows.
///
/// Deliberately **no `pdfaid` block**. That block is a claim of PDF/A
/// conformance, and this file does not have one (see the module docs). A
/// document that claimed it would be a document that lied to a validator.
#[must_use]
pub fn xmp(invoice: &EInvoice) -> String {
    let title = if invoice.number.is_empty() {
        "Invoice".to_owned()
    } else {
        format!("Invoice {}", invoice.number)
    };
    format!(
        r#"<?xpacket begin="{bom}" id="W5M0MpCehiHzreSzNTczkc9d"?>
<x:xmpmeta xmlns:x="adobe:ns:meta/">
  <rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#">
    <rdf:Description rdf:about="" xmlns:dc="http://purl.org/dc/elements/1.1/">
      <dc:title><rdf:Alt><rdf:li xml:lang="x-default">{title}</rdf:li></rdf:Alt></dc:title>
    </rdf:Description>
    <rdf:Description rdf:about="" xmlns:pdfaExtension="http://www.aiim.org/pdfa/ns/extension/" xmlns:pdfaSchema="http://www.aiim.org/pdfa/ns/schema#" xmlns:pdfaProperty="http://www.aiim.org/pdfa/ns/property#">
      <pdfaExtension:schemas>
        <rdf:Bag>
          <rdf:li rdf:parseType="Resource">
            <pdfaSchema:schema>Factur-X PDFA Extension Schema</pdfaSchema:schema>
            <pdfaSchema:namespaceURI>urn:factur-x:pdfa:CrossIndustryDocument:invoice:1p0#</pdfaSchema:namespaceURI>
            <pdfaSchema:prefix>fx</pdfaSchema:prefix>
            <pdfaSchema:property>
              <rdf:Seq>
                <rdf:li rdf:parseType="Resource">
                  <pdfaProperty:name>DocumentFileName</pdfaProperty:name>
                  <pdfaProperty:valueType>Text</pdfaProperty:valueType>
                  <pdfaProperty:category>external</pdfaProperty:category>
                  <pdfaProperty:description>name of the embedded XML invoice file</pdfaProperty:description>
                </rdf:li>
                <rdf:li rdf:parseType="Resource">
                  <pdfaProperty:name>DocumentType</pdfaProperty:name>
                  <pdfaProperty:valueType>Text</pdfaProperty:valueType>
                  <pdfaProperty:category>external</pdfaProperty:category>
                  <pdfaProperty:description>INVOICE</pdfaProperty:description>
                </rdf:li>
                <rdf:li rdf:parseType="Resource">
                  <pdfaProperty:name>Version</pdfaProperty:name>
                  <pdfaProperty:valueType>Text</pdfaProperty:valueType>
                  <pdfaProperty:category>external</pdfaProperty:category>
                  <pdfaProperty:description>the version of the Factur-X standard</pdfaProperty:description>
                </rdf:li>
                <rdf:li rdf:parseType="Resource">
                  <pdfaProperty:name>ConformanceLevel</pdfaProperty:name>
                  <pdfaProperty:valueType>Text</pdfaProperty:valueType>
                  <pdfaProperty:category>external</pdfaProperty:category>
                  <pdfaProperty:description>the conformance level of the embedded XML invoice</pdfaProperty:description>
                </rdf:li>
              </rdf:Seq>
            </pdfaSchema:property>
          </rdf:li>
        </rdf:Bag>
      </pdfaExtension:schemas>
    </rdf:Description>
    <rdf:Description rdf:about="" xmlns:fx="urn:factur-x:pdfa:CrossIndustryDocument:invoice:1p0#">
      <fx:DocumentType>INVOICE</fx:DocumentType>
      <fx:DocumentFileName>{name}</fx:DocumentFileName>
      <fx:Version>{version}</fx:Version>
      <fx:ConformanceLevel>{level}</fx:ConformanceLevel>
    </rdf:Description>
  </rdf:RDF>
</x:xmpmeta>
<?xpacket end="w"?>"#,
        bom = "\u{feff}",
        title = esc(&title),
        name = ATTACHMENT_NAME,
        version = FACTURX_VERSION,
        level = CONFORMANCE_LEVEL,
    )
}

// ---- the download ------------------------------------------------------------

/// The name the XML is saved under when it is downloaded on its own:
/// [`crate::billing_xml::file_name`] with this syntax's suffix.
#[must_use]
pub fn file_name(doc: &PrintDocument<'_>, s: &Strings) -> String {
    crate::billing_xml::file_name(doc, s, "factur-x")
}

// ---- formatting --------------------------------------------------------------

/// A date as UNTDID 2379 format 102: `YYYYMMDD`.
///
/// The one format CII and UBL do not share — an ISO date with hyphens is what
/// the other one writes ([`crate::billing_ubl`]).
fn date_102(value: time::Date) -> String {
    format!(
        "{:04}{:02}{:02}",
        value.year(),
        u8::from(value.month()),
        value.day()
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::billing_einvoice::{TaxCurrency, sample};

    #[test]
    fn every_string_in_the_document_is_escaped() {
        let mut invoice = sample();
        invoice.buyer.name = "Meier & Söhne <GmbH>".to_owned();
        invoice.note = "He said \"pay it\" — 'soon'".to_owned();
        invoice.lines[0].name = "Bolt <M8>".to_owned();
        let xml = render(&invoice);
        assert!(xml.contains("<ram:Name>Meier &amp; S\u{f6}hne &lt;GmbH&gt;</ram:Name>"));
        assert!(xml.contains("&quot;pay it&quot; \u{2014} &apos;soon&apos;"));
        assert!(xml.contains("<ram:Name>Bolt &lt;M8&gt;</ram:Name>"));
        // Nothing a customer typed can open an element.
        assert_eq!(xml.matches("<ram:Name>").count(), 3);
    }

    #[test]
    fn only_the_vat_total_carries_a_currency_attribute() {
        // Stating the currency on any other amount is a validation error in
        // the EN 16931 profile, not helpful redundancy.
        let xml = render(&sample());
        assert_eq!(xml.matches("currencyID=").count(), 1);
        assert!(xml.contains("<ram:TaxTotalAmount currencyID=\"EUR\">39.38</ram:TaxTotalAmount>"));
        assert!(xml.contains("<ram:GrandTotalAmount>226.88</ram:GrandTotalAmount>"));
    }

    #[test]
    fn a_foreign_currency_document_states_its_vat_twice() {
        let mut invoice = sample();
        invoice.currency = "USD".to_owned();
        invoice.tax_currency = Some(TaxCurrency {
            code: "EUR".to_owned(),
            tax_cents: 3_387,
        });
        let xml = render(&invoice);
        assert!(xml.contains("<ram:TaxCurrencyCode>EUR</ram:TaxCurrencyCode>"));
        assert!(xml.contains("<ram:InvoiceCurrencyCode>USD</ram:InvoiceCurrencyCode>"));
        assert!(xml.contains("<ram:TaxTotalAmount currencyID=\"USD\">39.38</ram:TaxTotalAmount>"));
        assert!(xml.contains("<ram:TaxTotalAmount currencyID=\"EUR\">33.87</ram:TaxTotalAmount>"));
        // …and the accounting currency is stated before the invoice currency,
        // which is the order the schema's sequence requires.
        let (tax_at, invoice_at) = (
            xml.find("TaxCurrencyCode").unwrap_or_default(),
            xml.find("InvoiceCurrencyCode").unwrap_or_default(),
        );
        assert!(tax_at < invoice_at);
    }

    #[test]
    fn a_credit_note_asks_for_no_money_and_names_what_it_corrects() {
        let mut credit = sample();
        credit.type_code = TypeCode::CreditNote;
        credit.preceding_invoice = "INV-2026-00001".to_owned();
        credit.payment_terms = String::new();
        let xml = render(&credit);
        assert!(xml.contains("<ram:TypeCode>381</ram:TypeCode>"));
        assert!(xml.contains("<ram:InvoiceReferencedDocument>"));
        assert!(xml.contains("<ram:IssuerAssignedID>INV-2026-00001</ram:IssuerAssignedID>"));
        // No bank details and no payment reference: nothing is payable on it.
        assert!(!xml.contains("IBANID"));
        assert!(!xml.contains("PaymentReference"));
    }

    #[test]
    fn the_xmp_describes_the_attachment_and_claims_no_pdfa_level() {
        let xmp = xmp(&sample());
        assert!(xmp.contains("<fx:DocumentFileName>factur-x.xml</fx:DocumentFileName>"));
        assert!(xmp.contains("<fx:ConformanceLevel>EN 16931</fx:ConformanceLevel>"));
        assert!(xmp.contains("<fx:DocumentType>INVOICE</fx:DocumentType>"));
        assert!(
            xmp.contains(
                "<dc:title><rdf:Alt><rdf:li xml:lang=\"x-default\">Invoice INV-2026-00001"
            )
        );
        // The claim we must not make: this file is not PDF/A yet.
        assert!(!xmp.contains("pdfaid:part"));
        assert!(xmp.starts_with("<?xpacket begin=\"\u{feff}\""));
        assert!(xmp.ends_with("<?xpacket end=\"w\"?>"));
    }

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

    #[test]
    fn the_document_is_well_formed_and_closes_everything_it_opens() {
        let xml = render(&sample());
        assert!(
            xml.starts_with(
                "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<rsm:CrossIndustryInvoice "
            )
        );
        assert!(xml.ends_with("</rsm:CrossIndustryInvoice>\n"));
        assert_eq!(unclosed(&xml), Vec::<String>::new());
        // …including a document with nothing optional on it at all.
        let mut bare = sample();
        bare.note = String::new();
        bare.buyer_reference = String::new();
        bare.payment_terms = String::new();
        bare.credit_transfer = None;
        bare.seller.legal_id = String::new();
        bare.seller.email = String::new();
        bare.buyer.email = String::new();
        bare.buyer.vat_id = String::new();
        bare.buyer.line2 = String::new();
        assert_eq!(unclosed(&render(&bare)), Vec::<String>::new());
    }
}
