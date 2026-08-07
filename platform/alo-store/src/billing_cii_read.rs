//! Reading UN/CEFACT CII — the syntax Factur-X and ZUGFeRD carry (alo Billing,
//! ADR 0035, wave B1.24).
//!
//! One job: walk a `CrossIndustryInvoice` tree
//! ([`crate::billing_xml_tree`]) into the syntax-free model
//! ([`crate::billing_einvoice_import::InboundInvoice`]), which is then checked
//! and stored. It decides nothing about what an invoice *is* — that is the
//! model's job, and keeping the two apart is what let a second syntax (UBL,
//! [`crate::billing_ubl_read`]) be added without re-deciding anything.
//!
//! Reading is more forgiving than writing, on purpose and only where being
//! strict would cost a real invoice:
//!
//! - **Optional terms that are absent are blank**, never an error. A supplier
//!   who states no note, no buyer reference and no bank account has still sent
//!   a valid invoice.
//! - **Both date spellings are accepted** (`20260807` and `2026-08-07`), even
//!   though CII prescribes the first.
//! - **A missing element and an empty one are the same thing**, which is what
//!   `text_at` already gives us.
//!
//! What it will *not* do is guess: a mandatory term that is absent, or a figure
//! that does not fit our units, is a refusal from the model with the business
//! term named.

use crate::billing_einvoice_import::{
    EInvoiceSyntax, InboundInvoice, InboundLine, InboundParty, StatedTotals, amount, category,
    date, describe, is_vat_scheme, quantity, rate_bp, unit_label,
};
use crate::billing_xml_tree::Element;
use crate::error::{Result, StoreError};

/// Reads a CII document into the semantic model.
///
/// # Errors
/// [`StoreError::Validation`] when a mandatory term is missing or a value
/// cannot be held in our units. Messages name the business term, never the
/// value.
pub(crate) fn read(root: &Element) -> Result<InboundInvoice> {
    let header = root.at(&["ExchangedDocument"]).ok_or_else(|| {
        StoreError::Validation(
            "this CII document states no ExchangedDocument, so it names neither a number nor a \
             date"
                .to_owned(),
        )
    })?;
    let transaction = root.at(&["SupplyChainTradeTransaction"]).ok_or_else(|| {
        StoreError::Validation(
            "this CII document states no SupplyChainTradeTransaction, so it has no lines, no \
             parties and no totals"
                .to_owned(),
        )
    })?;

    let type_code = header.text_at(&["TypeCode"]);
    let credit_note = is_credit_note(type_code)?;

    let agreement = transaction.at(&["ApplicableHeaderTradeAgreement"]);
    let settlement = transaction
        .at(&["ApplicableHeaderTradeSettlement"])
        .ok_or_else(|| {
            StoreError::Validation(
                "this CII document states no ApplicableHeaderTradeSettlement, so it has no \
                 currency and no totals"
                    .to_owned(),
            )
        })?;
    let summation = settlement
        .at(&["SpecifiedTradeSettlementHeaderMonetarySummation"])
        .ok_or_else(|| {
            StoreError::Validation(
                "BG-22: this document states no monetary summation, so it says nothing about what \
                 it is worth"
                    .to_owned(),
            )
        })?;

    let mut lines = Vec::new();
    for (index, item) in transaction
        .children_named("IncludedSupplyChainTradeLineItem")
        .enumerate()
    {
        lines.push(line(index + 1, item)?);
    }

    let currency = settlement.text_at(&["InvoiceCurrencyCode"]).to_uppercase();
    Ok(InboundInvoice {
        syntax: EInvoiceSyntax::Cii,
        credit_note,
        number: header.text_at(&["ID"]).to_owned(),
        issue_date: date("BT-2", header.text_at(&["IssueDateTime", "DateTimeString"]))?,
        due_date: optional_date(
            "BT-9",
            settlement.text_at(&[
                "SpecifiedTradePaymentTerms",
                "DueDateDateTime",
                "DateTimeString",
            ]),
        )?,
        buyer_reference: agreement
            .map_or("", |a| a.text_at(&["BuyerReference"]))
            .to_owned(),
        note: header.text_at(&["IncludedNote", "Content"]).to_owned(),
        payment_reference: settlement.text_at(&["PaymentReference"]).to_owned(),
        iban: settlement
            .text_at(&[
                "SpecifiedTradeSettlementPaymentMeans",
                "PayeePartyCreditorFinancialAccount",
                "IBANID",
            ])
            .to_owned(),
        seller: agreement
            .and_then(|a| a.at(&["SellerTradeParty"]))
            .map(party)
            .unwrap_or_default(),
        lines,
        totals: StatedTotals {
            line_total_cents: amount("BT-106", summation.text_at(&["LineTotalAmount"]))?,
            allowance_total_cents: optional_amount(
                "BT-107",
                summation.text_at(&["AllowanceTotalAmount"]),
            )?,
            charge_total_cents: optional_amount(
                "BT-108",
                summation.text_at(&["ChargeTotalAmount"]),
            )?,
            tax_exclusive_cents: amount("BT-109", summation.text_at(&["TaxBasisTotalAmount"]))?,
            // A document in a second accounting currency states its VAT twice
            // (BT-110 and BT-111, told apart by `currencyID`); the one in the
            // document's own currency is the one that counts, and picking it by
            // its currency rather than by its position is what keeps that true
            // whichever order a supplier writes them in.
            tax_total_cents: amount("BT-110", tax_total(summation, &currency))?,
            tax_inclusive_cents: amount("BT-112", summation.text_at(&["GrandTotalAmount"]))?,
            prepaid_cents: optional_amount("BT-113", summation.text_at(&["TotalPrepaidAmount"]))?,
            rounding_cents: optional_amount("BT-114", summation.text_at(&["RoundingAmount"]))?,
            payable_cents: amount("BT-115", summation.text_at(&["DuePayableAmount"]))?,
        },
        currency,
    })
}

/// The VAT total in the document's own currency (BT-110), out of the one or two
/// the summation states.
///
/// A summation that states only one figure is the ordinary case and that figure
/// is it, `currencyID` or not.
fn tax_total<'a>(summation: &'a Element, currency: &str) -> &'a str {
    let mut totals = summation.children_named("TaxTotalAmount");
    let first = totals.next();
    let matching = first
        .into_iter()
        .chain(totals)
        .find(|total| total.attr("currencyID").eq_ignore_ascii_case(currency));
    matching.or(first).map_or("", |total| total.text.as_str())
}

/// Whether the document type code (BT-3) names a credit note.
///
/// Only the two codes our own documents are: an inbound 384 (corrected
/// invoice), 389 (self-billed) or 875 (partial construction invoice) is a real
/// document type we would be guessing about, and guessing wrong puts money in
/// the ledger the wrong way round.
fn is_credit_note(code: &str) -> Result<bool> {
    match code.trim() {
        "380" => Ok(false),
        "381" => Ok(true),
        "" => Err(StoreError::Validation(
            "BT-3: the document states no type code, so it does not say whether it is an invoice \
             or a credit note"
                .to_owned(),
        )),
        other => Err(StoreError::Validation(format!(
            "BT-3: document type {other} is not one alo can store. Only a commercial invoice (380) \
             and a credit note (381) can be booked as a bill"
        ))),
    }
}

/// A party (BG-4) as CII states it.
fn party(element: &Element) -> InboundParty {
    let address = element.at(&["PostalTradeAddress"]);
    let text = |path: &[&str]| address.map_or("", |a| a.text_at(path)).to_owned();
    InboundParty {
        name: element.text_at(&["Name"]).to_owned(),
        // A party may state several registrations (VAT and a tax number); the
        // one scheme `VA` names is the VAT identifier.
        vat_id: element
            .children_named("SpecifiedTaxRegistration")
            .filter_map(|reg| reg.child("ID"))
            .find(|id| is_vat_scheme(id))
            .map(|id| id.text.clone())
            .unwrap_or_default(),
        legal_id: element
            .text_at(&["SpecifiedLegalOrganization", "ID"])
            .to_owned(),
        line1: text(&["LineOne"]),
        line2: text(&["LineTwo"]),
        postal_code: text(&["PostcodeCode"]),
        city: text(&["CityName"]),
        country: text(&["CountryID"]).to_uppercase(),
        email: element
            .children_named("URIUniversalCommunication")
            .filter_map(|uri| uri.child("URIID"))
            .find(|id| id.attr("schemeID").eq_ignore_ascii_case("EM"))
            .map(|id| id.text.clone())
            .unwrap_or_default(),
    }
}

/// One line (BG-25) as CII states it.
fn line(position: usize, item: &Element) -> Result<InboundLine> {
    let product = item.at(&["SpecifiedTradeProduct"]);
    let agreement = item.at(&["SpecifiedLineTradeAgreement"]);
    let delivery = item.at(&["SpecifiedLineTradeDelivery"]);
    let settlement = item.at(&["SpecifiedLineTradeSettlement"]);

    let billed = delivery
        .and_then(|d| d.at(&["BilledQuantity"]))
        .ok_or_else(|| {
            StoreError::Validation(format!(
                "line {position}: BT-129 — the line states no quantity"
            ))
        })?;
    let price = agreement
        .and_then(|a| a.at(&["NetPriceProductTradePrice"]))
        .ok_or_else(|| {
            StoreError::Validation(format!(
                "line {position}: BT-146 — the line states no net price"
            ))
        })?;
    // A price stated per a base quantity ("€80.00 per 100 pieces") multiplies
    // the whole line, and our line model holds one price per one unit.
    if let Some(base) = price.child("BasisQuantity") {
        let base = quantity("BT-149", &base.text)?;
        if base != 1_000 {
            return Err(StoreError::Validation(format!(
                "line {position}: BT-149 — the price is stated per a base quantity other than one, \
                 which cannot be stored as one line"
            )));
        }
    }

    let tax = settlement.and_then(|s| s.at(&["ApplicableTradeTax"]));
    category(position, tax.map_or("", |t| t.text_at(&["CategoryCode"])))?;

    Ok(InboundLine {
        description: describe(
            product.map_or("", |p| p.text_at(&["Name"])),
            product.map_or("", |p| p.text_at(&["Description"])),
        ),
        unit: unit_label(billed.attr("unitCode")),
        qty_milli: quantity("BT-129", &billed.text)?,
        unit_price_cents: amount("BT-146", price.text_at(&["ChargeAmount"]))?,
        vat_rate_bp: rate_bp(
            "BT-152",
            tax.map_or("0", |t| t.text_at(&["RateApplicablePercent"])),
        )?,
        net_cents: amount(
            "BT-131",
            settlement.map_or("", |s| {
                s.text_at(&[
                    "SpecifiedTradeSettlementLineMonetarySummation",
                    "LineTotalAmount",
                ])
            }),
        )?,
    })
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
    fn a_type_code_that_is_not_a_bill_is_refused_by_its_code() {
        assert_eq!(is_credit_note("380").ok(), Some(false));
        assert_eq!(is_credit_note(" 381 ").ok(), Some(true));
        assert!(refused(is_credit_note("")).contains("BT-3"));
        let message = refused(is_credit_note("384"));
        assert!(message.contains("384"), "{message}");
    }

    #[test]
    fn a_party_reads_its_vat_id_from_the_registration_that_names_the_scheme() {
        let element = tree(
            r#"<SellerTradeParty>
                 <Name>Lieferant GmbH</Name>
                 <SpecifiedLegalOrganization><ID>HRB 1234</ID></SpecifiedLegalOrganization>
                 <PostalTradeAddress>
                   <PostcodeCode>10115</PostcodeCode>
                   <LineOne>Hauptstraße 5</LineOne>
                   <LineTwo>Gebäude C</LineTwo>
                   <CityName>Berlin</CityName>
                   <CountryID>de</CountryID>
                 </PostalTradeAddress>
                 <URIUniversalCommunication>
                   <URIID schemeID="EM">rechnung@lieferant.test</URIID>
                 </URIUniversalCommunication>
                 <SpecifiedTaxRegistration><ID schemeID="FC">12/345/67890</ID></SpecifiedTaxRegistration>
                 <SpecifiedTaxRegistration><ID schemeID="VA">DE811907980</ID></SpecifiedTaxRegistration>
               </SellerTradeParty>"#,
        );
        let party = party(&element);
        assert_eq!(party.name, "Lieferant GmbH");
        assert_eq!(party.vat_id, "DE811907980", "the FC tax number is not it");
        assert_eq!(party.legal_id, "HRB 1234");
        assert_eq!(party.line1, "Hauptstraße 5");
        assert_eq!(party.line2, "Gebäude C");
        assert_eq!(party.postal_code, "10115");
        assert_eq!(party.city, "Berlin");
        assert_eq!(party.country, "DE", "the code is canonical, not as typed");
        assert_eq!(party.email, "rechnung@lieferant.test");
    }

    #[test]
    fn a_party_that_states_almost_nothing_still_reads() {
        let party = party(&tree(
            "<SellerTradeParty><Name>Sole Trader</Name></SellerTradeParty>",
        ));
        assert_eq!(party.name, "Sole Trader");
        assert_eq!(party.vat_id, "", "a B2C supplier has none");
        assert_eq!(party.country, "");
    }

    #[test]
    fn a_line_reads_its_quantity_price_rate_and_stated_amount() {
        let item = tree(
            r#"<IncludedSupplyChainTradeLineItem>
                 <SpecifiedTradeProduct><Name>Consulting</Name><Description>March</Description></SpecifiedTradeProduct>
                 <SpecifiedLineTradeAgreement>
                   <NetPriceProductTradePrice><ChargeAmount>125.00</ChargeAmount></NetPriceProductTradePrice>
                 </SpecifiedLineTradeAgreement>
                 <SpecifiedLineTradeDelivery><BilledQuantity unitCode="HUR">15</BilledQuantity></SpecifiedLineTradeDelivery>
                 <SpecifiedLineTradeSettlement>
                   <ApplicableTradeTax><CategoryCode>S</CategoryCode><RateApplicablePercent>21.00</RateApplicablePercent></ApplicableTradeTax>
                   <SpecifiedTradeSettlementLineMonetarySummation><LineTotalAmount>1875.00</LineTotalAmount></SpecifiedTradeSettlementLineMonetarySummation>
                 </SpecifiedLineTradeSettlement>
               </IncludedSupplyChainTradeLineItem>"#,
        );
        let line = line(1, &item).unwrap_or_else(|e| panic!("rejected: {e}"));
        assert_eq!(line.description, "Consulting\nMarch");
        assert_eq!(line.unit, "hour");
        assert_eq!(line.qty_milli, 15_000);
        assert_eq!(line.unit_price_cents, 12_500);
        assert_eq!(line.vat_rate_bp, 2100);
        assert_eq!(line.net_cents, 187_500);
    }

    #[test]
    fn a_line_that_prices_per_a_hundred_pieces_is_refused_rather_than_scaled() {
        let item = tree(
            r#"<IncludedSupplyChainTradeLineItem>
                 <SpecifiedTradeProduct><Name>Screws</Name></SpecifiedTradeProduct>
                 <SpecifiedLineTradeAgreement>
                   <NetPriceProductTradePrice>
                     <ChargeAmount>80.00</ChargeAmount>
                     <BasisQuantity unitCode="H87">100</BasisQuantity>
                   </NetPriceProductTradePrice>
                 </SpecifiedLineTradeAgreement>
                 <SpecifiedLineTradeDelivery><BilledQuantity unitCode="H87">500</BilledQuantity></SpecifiedLineTradeDelivery>
                 <SpecifiedLineTradeSettlement>
                   <ApplicableTradeTax><CategoryCode>S</CategoryCode><RateApplicablePercent>21.00</RateApplicablePercent></ApplicableTradeTax>
                   <SpecifiedTradeSettlementLineMonetarySummation><LineTotalAmount>400.00</LineTotalAmount></SpecifiedTradeSettlementLineMonetarySummation>
                 </SpecifiedLineTradeSettlement>
               </IncludedSupplyChainTradeLineItem>"#,
        );
        let message = refused(line(2, &item));
        assert!(
            message.contains("line 2") && message.contains("BT-149"),
            "{message}"
        );
        // A base quantity of exactly one is the ordinary case and passes.
        let plain = tree(
            r#"<IncludedSupplyChainTradeLineItem>
                 <SpecifiedTradeProduct><Name>Screws</Name></SpecifiedTradeProduct>
                 <SpecifiedLineTradeAgreement>
                   <NetPriceProductTradePrice>
                     <ChargeAmount>0.80</ChargeAmount>
                     <BasisQuantity unitCode="H87">1</BasisQuantity>
                   </NetPriceProductTradePrice>
                 </SpecifiedLineTradeAgreement>
                 <SpecifiedLineTradeDelivery><BilledQuantity unitCode="H87">500</BilledQuantity></SpecifiedLineTradeDelivery>
                 <SpecifiedLineTradeSettlement>
                   <ApplicableTradeTax><CategoryCode>S</CategoryCode><RateApplicablePercent>21.00</RateApplicablePercent></ApplicableTradeTax>
                   <SpecifiedTradeSettlementLineMonetarySummation><LineTotalAmount>400.00</LineTotalAmount></SpecifiedTradeSettlementLineMonetarySummation>
                 </SpecifiedLineTradeSettlement>
               </IncludedSupplyChainTradeLineItem>"#,
        );
        assert!(line(1, &plain).is_ok());
    }

    #[test]
    fn a_line_missing_its_quantity_or_price_names_which() {
        let no_quantity = tree(
            r#"<IncludedSupplyChainTradeLineItem>
                 <SpecifiedLineTradeAgreement>
                   <NetPriceProductTradePrice><ChargeAmount>1.00</ChargeAmount></NetPriceProductTradePrice>
                 </SpecifiedLineTradeAgreement>
               </IncludedSupplyChainTradeLineItem>"#,
        );
        assert!(refused(line(1, &no_quantity)).contains("BT-129"));
        let no_price = tree(
            r#"<IncludedSupplyChainTradeLineItem>
                 <SpecifiedLineTradeDelivery><BilledQuantity unitCode="C62">1</BilledQuantity></SpecifiedLineTradeDelivery>
               </IncludedSupplyChainTradeLineItem>"#,
        );
        assert!(refused(line(1, &no_price)).contains("BT-146"));
    }

    #[test]
    fn a_document_missing_a_whole_group_says_which_group() {
        assert!(refused(read(&tree("<CrossIndustryInvoice/>"))).contains("ExchangedDocument"));
        let no_transaction = tree(
            r#"<CrossIndustryInvoice><ExchangedDocument><ID>1</ID><TypeCode>380</TypeCode>
                 <IssueDateTime><DateTimeString format="102">20260807</DateTimeString></IssueDateTime>
               </ExchangedDocument></CrossIndustryInvoice>"#,
        );
        assert!(refused(read(&no_transaction)).contains("SupplyChainTradeTransaction"));
    }
}
