//! Receiving an e-invoice (alo Billing, wave B1.24): a supplier's Factur-X or
//! XRechnung file becoming a bill, the refusals that keep an incoherent
//! document out of the ledger, the one-way approval door, and the tenancy proof
//! (Law 1: isolation is tested, not assumed).
//!
//! Every figure asserted here is hand-computed from the fixture, so a change in
//! our arithmetic fails the suite rather than moving the expectation with it.
//!
//! Runs against the real Postgres from compose (see `tests/common`).
#![allow(clippy::unwrap_used, clippy::expect_used)]

use crate::common;

use alo_store::{
    AccountStore, BillStatus, BillTotals, BillingBillId, EInvoiceSyntax, Store, StoreError,
    TenantId,
};
use sqlx::PgPool;
use sqlx::postgres::PgPoolOptions;

/// Asserts a result is the clean not-found denial — never data, never an
/// internal (`Db`) error.
fn assert_not_found<T: std::fmt::Debug>(result: Result<T, StoreError>) {
    match result {
        Err(StoreError::NotFound) => {}
        Err(other) => panic!("expected NotFound, got: {other:?}"),
        Ok(value) => panic!("expected NotFound, but got data: {value:?}"),
    }
}

/// Asserts a result is the typed state refusal, returning its message.
fn assert_conflict<T: std::fmt::Debug>(result: Result<T, StoreError>) -> String {
    match result {
        Err(StoreError::Conflict(message)) => message,
        other => panic!("expected Conflict, got: {other:?}"),
    }
}

/// Asserts a result is the typed input refusal, returning its message.
fn assert_validation<T: std::fmt::Debug>(result: Result<T, StoreError>) -> String {
    match result {
        Err(StoreError::Validation(message)) => message,
        other => panic!("expected Validation, got: {other:?}"),
    }
}

/// A tenant with one user, returning the account door and the tenant id.
async fn tenant(store: &Store, tag: &str) -> (AccountStore, TenantId) {
    let tenant = store.create_tenant(&format!("bill-{tag}")).await.unwrap();
    let user = store
        .for_tenant(tenant.clone())
        .create_user(&format!("{tag}@bills.test"))
        .await
        .unwrap();
    (store.for_account(tenant.clone(), user), tenant)
}

/// A raw pool alongside the store, for counting rows the store's own reads
/// would filter by tenant.
async fn raw_pool() -> PgPool {
    PgPoolOptions::new()
        .max_connections(2)
        .connect(&common::database_url())
        .await
        .unwrap()
}

/// A supplier's Factur-X (CII) invoice, as a supplier's system would send it.
///
/// Hand-computed: 8 h at €125.00 = €1000.00, plus 240 km at €0.42 = €100.80.
/// Line total €1100.80; VAT at 21 % of that is €231.168, which rounds once at
/// the rate subtotal to €231.17; gross €1331.97.
fn facturx(number: &str) -> String {
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<rsm:CrossIndustryInvoice xmlns:rsm="urn:un:unece:uncefact:data:standard:CrossIndustryInvoice:100" xmlns:ram="urn:un:unece:uncefact:data:standard:ReusableAggregateBusinessInformationEntity:100" xmlns:udt="urn:un:unece:uncefact:data:standard:UnqualifiedDataType:100">
  <rsm:ExchangedDocumentContext>
    <ram:GuidelineSpecifiedDocumentContextParameter><ram:ID>urn:cen.eu:en16931:2017</ram:ID></ram:GuidelineSpecifiedDocumentContextParameter>
  </rsm:ExchangedDocumentContext>
  <rsm:ExchangedDocument>
    <ram:ID>{number}</ram:ID>
    <ram:TypeCode>380</ram:TypeCode>
    <ram:IssueDateTime><udt:DateTimeString format="102">20260807</udt:DateTimeString></ram:IssueDateTime>
    <ram:IncludedNote><ram:Content>Danke für Ihren Auftrag.</ram:Content></ram:IncludedNote>
  </rsm:ExchangedDocument>
  <rsm:SupplyChainTradeTransaction>
    <ram:IncludedSupplyChainTradeLineItem>
      <ram:AssociatedDocumentLineDocument><ram:LineID>1</ram:LineID></ram:AssociatedDocumentLineDocument>
      <ram:SpecifiedTradeProduct><ram:Name>Beratung</ram:Name><ram:Description>August, vor Ort</ram:Description></ram:SpecifiedTradeProduct>
      <ram:SpecifiedLineTradeAgreement><ram:NetPriceProductTradePrice><ram:ChargeAmount>125.00</ram:ChargeAmount></ram:NetPriceProductTradePrice></ram:SpecifiedLineTradeAgreement>
      <ram:SpecifiedLineTradeDelivery><ram:BilledQuantity unitCode="HUR">8</ram:BilledQuantity></ram:SpecifiedLineTradeDelivery>
      <ram:SpecifiedLineTradeSettlement>
        <ram:ApplicableTradeTax><ram:TypeCode>VAT</ram:TypeCode><ram:CategoryCode>S</ram:CategoryCode><ram:RateApplicablePercent>21.00</ram:RateApplicablePercent></ram:ApplicableTradeTax>
        <ram:SpecifiedTradeSettlementLineMonetarySummation><ram:LineTotalAmount>1000.00</ram:LineTotalAmount></ram:SpecifiedTradeSettlementLineMonetarySummation>
      </ram:SpecifiedLineTradeSettlement>
    </ram:IncludedSupplyChainTradeLineItem>
    <ram:IncludedSupplyChainTradeLineItem>
      <ram:AssociatedDocumentLineDocument><ram:LineID>2</ram:LineID></ram:AssociatedDocumentLineDocument>
      <ram:SpecifiedTradeProduct><ram:Name>Fahrtkosten</ram:Name></ram:SpecifiedTradeProduct>
      <ram:SpecifiedLineTradeAgreement><ram:NetPriceProductTradePrice><ram:ChargeAmount>0.42</ram:ChargeAmount></ram:NetPriceProductTradePrice></ram:SpecifiedLineTradeAgreement>
      <ram:SpecifiedLineTradeDelivery><ram:BilledQuantity unitCode="KMT">240</ram:BilledQuantity></ram:SpecifiedLineTradeDelivery>
      <ram:SpecifiedLineTradeSettlement>
        <ram:ApplicableTradeTax><ram:TypeCode>VAT</ram:TypeCode><ram:CategoryCode>S</ram:CategoryCode><ram:RateApplicablePercent>21.00</ram:RateApplicablePercent></ram:ApplicableTradeTax>
        <ram:SpecifiedTradeSettlementLineMonetarySummation><ram:LineTotalAmount>100.80</ram:LineTotalAmount></ram:SpecifiedTradeSettlementLineMonetarySummation>
      </ram:SpecifiedLineTradeSettlement>
    </ram:IncludedSupplyChainTradeLineItem>
    <ram:ApplicableHeaderTradeAgreement>
      <ram:BuyerReference>PO-2026-4</ram:BuyerReference>
      <ram:SellerTradeParty>
        <ram:Name>Lieferant GmbH</ram:Name>
        <ram:SpecifiedLegalOrganization><ram:ID>HRB 1234</ram:ID></ram:SpecifiedLegalOrganization>
        <ram:PostalTradeAddress>
          <ram:PostcodeCode>10115</ram:PostcodeCode>
          <ram:LineOne>Hauptstraße 5</ram:LineOne>
          <ram:CityName>Berlin</ram:CityName>
          <ram:CountryID>DE</ram:CountryID>
        </ram:PostalTradeAddress>
        <ram:URIUniversalCommunication><ram:URIID schemeID="EM">rechnung@lieferant.test</ram:URIID></ram:URIUniversalCommunication>
        <ram:SpecifiedTaxRegistration><ram:ID schemeID="VA">DE811907980</ram:ID></ram:SpecifiedTaxRegistration>
      </ram:SellerTradeParty>
      <ram:BuyerTradeParty><ram:Name>Alo Werkplaats B.V.</ram:Name></ram:BuyerTradeParty>
    </ram:ApplicableHeaderTradeAgreement>
    <ram:ApplicableHeaderTradeDelivery/>
    <ram:ApplicableHeaderTradeSettlement>
      <ram:PaymentReference>{number}</ram:PaymentReference>
      <ram:InvoiceCurrencyCode>EUR</ram:InvoiceCurrencyCode>
      <ram:SpecifiedTradeSettlementPaymentMeans>
        <ram:TypeCode>30</ram:TypeCode>
        <ram:PayeePartyCreditorFinancialAccount><ram:IBANID>DE02120300000000202051</ram:IBANID></ram:PayeePartyCreditorFinancialAccount>
      </ram:SpecifiedTradeSettlementPaymentMeans>
      <ram:ApplicableTradeTax>
        <ram:CalculatedAmount>231.17</ram:CalculatedAmount>
        <ram:TypeCode>VAT</ram:TypeCode>
        <ram:BasisAmount>1100.80</ram:BasisAmount>
        <ram:CategoryCode>S</ram:CategoryCode>
        <ram:RateApplicablePercent>21.00</ram:RateApplicablePercent>
      </ram:ApplicableTradeTax>
      <ram:SpecifiedTradePaymentTerms>
        <ram:Description>Zahlbar innerhalb von 30 Tagen.</ram:Description>
        <ram:DueDateDateTime><udt:DateTimeString format="102">20260906</udt:DateTimeString></ram:DueDateDateTime>
      </ram:SpecifiedTradePaymentTerms>
      <ram:SpecifiedTradeSettlementHeaderMonetarySummation>
        <ram:LineTotalAmount>1100.80</ram:LineTotalAmount>
        <ram:TaxBasisTotalAmount>1100.80</ram:TaxBasisTotalAmount>
        <ram:TaxTotalAmount currencyID="EUR">231.17</ram:TaxTotalAmount>
        <ram:GrandTotalAmount>1331.97</ram:GrandTotalAmount>
        <ram:DuePayableAmount>1331.97</ram:DuePayableAmount>
      </ram:SpecifiedTradeSettlementHeaderMonetarySummation>
    </ram:ApplicableHeaderTradeSettlement>
  </rsm:SupplyChainTradeTransaction>
</rsm:CrossIndustryInvoice>"#
    )
}

/// The same supplier's XRechnung (UBL) **credit note**: 2 h of the consulting
/// above given back. €250.00 net, €52.50 VAT, €302.50 gross — stated positive
/// under type 381, as the standard requires.
fn xrechnung_credit_note(number: &str) -> String {
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<ubl:CreditNote xmlns:ubl="urn:oasis:names:specification:ubl:schema:xsd:CreditNote-2" xmlns:cac="urn:oasis:names:specification:ubl:schema:xsd:CommonAggregateComponents-2" xmlns:cbc="urn:oasis:names:specification:ubl:schema:xsd:CommonBasicComponents-2">
  <cbc:CustomizationID>urn:cen.eu:en16931:2017#compliant#urn:xoev-de:kosit:standard:xrechnung_3.0</cbc:CustomizationID>
  <cbc:ID>{number}</cbc:ID>
  <cbc:IssueDate>2026-08-20</cbc:IssueDate>
  <cbc:CreditNoteTypeCode>381</cbc:CreditNoteTypeCode>
  <cbc:DocumentCurrencyCode>EUR</cbc:DocumentCurrencyCode>
  <cbc:BuyerReference>PO-2026-4</cbc:BuyerReference>
  <cac:BillingReference><cac:InvoiceDocumentReference><cbc:ID>R-2026-77</cbc:ID></cac:InvoiceDocumentReference></cac:BillingReference>
  <cac:AccountingSupplierParty>
    <cac:Party>
      <cbc:EndpointID schemeID="EM">rechnung@lieferant.test</cbc:EndpointID>
      <cac:PostalAddress>
        <cbc:StreetName>Hauptstraße 5</cbc:StreetName>
        <cbc:CityName>Berlin</cbc:CityName>
        <cbc:PostalZone>10115</cbc:PostalZone>
        <cac:Country><cbc:IdentificationCode>DE</cbc:IdentificationCode></cac:Country>
      </cac:PostalAddress>
      <cac:PartyTaxScheme><cbc:CompanyID>DE811907980</cbc:CompanyID><cac:TaxScheme><cbc:ID>VAT</cbc:ID></cac:TaxScheme></cac:PartyTaxScheme>
      <cac:PartyLegalEntity><cbc:RegistrationName>Lieferant GmbH</cbc:RegistrationName><cbc:CompanyID>HRB 1234</cbc:CompanyID></cac:PartyLegalEntity>
    </cac:Party>
  </cac:AccountingSupplierParty>
  <cac:AccountingCustomerParty><cac:Party><cac:PartyLegalEntity><cbc:RegistrationName>Alo Werkplaats B.V.</cbc:RegistrationName></cac:PartyLegalEntity></cac:Party></cac:AccountingCustomerParty>
  <cac:TaxTotal>
    <cbc:TaxAmount currencyID="EUR">52.50</cbc:TaxAmount>
    <cac:TaxSubtotal>
      <cbc:TaxableAmount currencyID="EUR">250.00</cbc:TaxableAmount>
      <cbc:TaxAmount currencyID="EUR">52.50</cbc:TaxAmount>
      <cac:TaxCategory><cbc:ID>S</cbc:ID><cbc:Percent>21.00</cbc:Percent><cac:TaxScheme><cbc:ID>VAT</cbc:ID></cac:TaxScheme></cac:TaxCategory>
    </cac:TaxSubtotal>
  </cac:TaxTotal>
  <cac:LegalMonetaryTotal>
    <cbc:LineExtensionAmount currencyID="EUR">250.00</cbc:LineExtensionAmount>
    <cbc:TaxExclusiveAmount currencyID="EUR">250.00</cbc:TaxExclusiveAmount>
    <cbc:TaxInclusiveAmount currencyID="EUR">302.50</cbc:TaxInclusiveAmount>
    <cbc:PayableAmount currencyID="EUR">302.50</cbc:PayableAmount>
  </cac:LegalMonetaryTotal>
  <cac:CreditNoteLine>
    <cbc:ID>1</cbc:ID>
    <cbc:CreditedQuantity unitCode="HUR">2</cbc:CreditedQuantity>
    <cbc:LineExtensionAmount currencyID="EUR">250.00</cbc:LineExtensionAmount>
    <cac:Item>
      <cbc:Name>Beratung</cbc:Name>
      <cac:ClassifiedTaxCategory><cbc:ID>S</cbc:ID><cbc:Percent>21.00</cbc:Percent><cac:TaxScheme><cbc:ID>VAT</cbc:ID></cac:TaxScheme></cac:ClassifiedTaxCategory>
    </cac:Item>
    <cac:Price><cbc:PriceAmount currencyID="EUR">125.00</cbc:PriceAmount></cac:Price>
  </cac:CreditNoteLine>
</ubl:CreditNote>"#
    )
}

#[tokio::test]
async fn a_suppliers_facturx_invoice_becomes_a_bill_with_their_own_figures() {
    let store = common::test_store().await;
    let (account, _tenant) = tenant(&store, "fx").await;

    let id = account
        .import_billing_bill(facturx("R-2026-77").as_bytes())
        .await
        .unwrap();
    let document = account.billing_bill(&id).await.unwrap().unwrap();
    let bill = &document.bill;

    assert_eq!(bill.source_syntax, Some(EInvoiceSyntax::Cii));
    assert_eq!(bill.source_sha256.len(), 64);
    assert!(!bill.credit_note);
    assert_eq!(bill.status, BillStatus::Received, "nobody has decided yet");
    assert_eq!(bill.number, "R-2026-77", "their number, not one of ours");
    assert_eq!(bill.issue_date.to_string(), "2026-08-07");
    assert_eq!(
        bill.due_date.map(|d| d.to_string()).as_deref(),
        Some("2026-09-06")
    );
    assert_eq!(bill.currency, "EUR");
    assert_eq!(bill.buyer_reference, "PO-2026-4");
    assert_eq!(bill.note, "Danke für Ihren Auftrag.");
    assert_eq!(bill.payment_reference, "R-2026-77");
    assert_eq!(bill.supplier.name, "Lieferant GmbH");
    assert_eq!(bill.supplier.vat_id, "DE811907980");
    assert_eq!(bill.supplier.legal_id, "HRB 1234");
    assert_eq!(bill.supplier.line1, "Hauptstraße 5");
    assert_eq!(bill.supplier.postal_code, "10115");
    assert_eq!(bill.supplier.city, "Berlin");
    assert_eq!(bill.supplier.country, "DE");
    assert_eq!(bill.supplier.email, "rechnung@lieferant.test");
    assert_eq!(bill.supplier.iban, "DE02120300000000202051");

    // The stated totals, hand-computed above, carried across exactly.
    assert_eq!(
        bill.totals,
        BillTotals {
            line_total_cents: 110_080,
            allowance_total_cents: 0,
            charge_total_cents: 0,
            tax_exclusive_cents: 110_080,
            tax_total_cents: 23_117,
            tax_inclusive_cents: 133_197,
            prepaid_cents: 0,
            payable_cents: 133_197,
        }
    );
    // …and our own arithmetic over the stored lines agrees with them, which is
    // what the import refuses to store without.
    assert_eq!(document.computed.net_cents, 110_080);
    assert_eq!(document.computed.vat_cents, 23_117);
    assert_eq!(document.computed.gross_cents, 133_197);

    assert_eq!(document.lines.len(), 2);
    let first = &document.lines[0];
    assert_eq!(first.description, "Beratung\nAugust, vor Ort");
    assert_eq!(first.unit, "hour", "HUR read back as a word");
    assert_eq!(first.qty_milli, 8_000);
    assert_eq!(first.unit_price_cents, 12_500);
    assert_eq!(first.vat_rate_bp, 2100);
    let second = &document.lines[1];
    assert_eq!(second.description, "Fahrtkosten");
    assert_eq!(second.unit, "km");
    assert_eq!(second.qty_milli, 240_000);
    assert_eq!(second.unit_price_cents, 42);

    // It is on the approval queue, and on no other.
    let waiting = account
        .billing_bills(Some(BillStatus::Received))
        .await
        .unwrap();
    assert_eq!(waiting.len(), 1);
    assert_eq!(waiting[0].id.as_str(), id.as_str());
    assert!(
        account
            .billing_bills(Some(BillStatus::Approved))
            .await
            .unwrap()
            .is_empty()
    );
    assert_eq!(account.billing_bills(None).await.unwrap().len(), 1);
}

#[tokio::test]
async fn a_credit_note_is_stored_the_way_our_own_ledger_holds_one() {
    let store = common::test_store().await;
    let (account, _tenant) = tenant(&store, "cn").await;

    let invoice = account
        .import_billing_bill(facturx("R-2026-77").as_bytes())
        .await
        .unwrap();
    let credit = account
        .import_billing_bill(xrechnung_credit_note("G-2026-9").as_bytes())
        .await
        .unwrap();

    let credit = account.billing_bill(&credit).await.unwrap().unwrap();
    assert_eq!(credit.bill.source_syntax, Some(EInvoiceSyntax::Ubl));
    assert!(credit.bill.credit_note);
    // The file states €302.50 positive under type 381; the ledger holds it
    // negative, as it holds our own credit notes.
    assert_eq!(credit.bill.totals.line_total_cents, -25_000);
    assert_eq!(credit.bill.totals.tax_total_cents, -5_250);
    assert_eq!(credit.bill.totals.payable_cents, -30_250);
    assert_eq!(credit.lines.len(), 1);
    assert_eq!(credit.lines[0].qty_milli, -2_000, "the quantity is negated");
    assert_eq!(
        credit.lines[0].unit_price_cents, 12_500,
        "a price is never negated"
    );
    assert_eq!(credit.computed.gross_cents, -30_250);

    // What a bookkeeper actually needs: the two documents together are what is
    // owed, and crediting part of an invoice leaves the rest.
    let invoice = account.billing_bill(&invoice).await.unwrap().unwrap();
    assert_eq!(
        invoice.bill.totals.payable_cents + credit.bill.totals.payable_cents,
        102_947
    );
    // Both are the same supplier, so they list together.
    let all = account.billing_bills(None).await.unwrap();
    assert_eq!(all.len(), 2);
    assert!(all.iter().all(|bill| bill.supplier.key() == "DE811907980"));
    // Newest document first: the credit note is dated later.
    assert_eq!(all[0].number, "G-2026-9");
}

#[tokio::test]
async fn the_same_document_is_never_booked_twice() {
    let store = common::test_store().await;
    let (account, _tenant) = tenant(&store, "dup").await;

    account
        .import_billing_bill(facturx("R-2026-77").as_bytes())
        .await
        .unwrap();
    // The identical file again — the forwarded-twice case.
    let message = assert_conflict(
        account
            .import_billing_bill(facturx("R-2026-77").as_bytes())
            .await,
    );
    assert!(message.contains("already been imported"), "{message}");
    // …and so is a re-export of the same document that differs byte for byte
    // (a note added, a different date stamp): the identity is the supplier and
    // their number, not the checksum.
    let re_exported = facturx("R-2026-77").replace("Danke für Ihren Auftrag.", "Vielen Dank.");
    assert_conflict(account.import_billing_bill(re_exported.as_bytes()).await);
    assert_eq!(account.billing_bills(None).await.unwrap().len(), 1);

    // A different number from the same supplier is a different document.
    account
        .import_billing_bill(facturx("R-2026-78").as_bytes())
        .await
        .unwrap();
    assert_eq!(account.billing_bills(None).await.unwrap().len(), 2);

    // And the same number from a *different* supplier is also a different
    // document — numbers are unique within a supplier, not across them.
    let other_supplier = facturx("R-2026-77")
        .replace("Lieferant GmbH", "Zweite GmbH")
        .replace("DE811907980", "DE136695976");
    account
        .import_billing_bill(other_supplier.as_bytes())
        .await
        .unwrap();
    assert_eq!(account.billing_bills(None).await.unwrap().len(), 3);
}

#[tokio::test]
async fn a_document_that_does_not_add_up_is_refused_and_nothing_is_written() {
    let store = common::test_store().await;
    let (account, tenant_id) = tenant(&store, "bad").await;
    let pool = raw_pool().await;

    // The dangerous case: a gross that does not follow from the net and the
    // VAT. Booked, it would be paid at the wrong figure.
    let tampered = facturx("R-2026-77").replace(
        "<ram:GrandTotalAmount>1331.97</ram:GrandTotalAmount>",
        "<ram:GrandTotalAmount>1391.97</ram:GrandTotalAmount>",
    );
    let message = assert_validation(account.import_billing_bill(tampered.as_bytes()).await);
    assert!(message.contains("BR-CO-15"), "{message}");
    assert!(
        message.contains("6000 cents"),
        "the gap is named: {message}"
    );

    // A line whose stated amount is not quantity × price — what a line-level
    // discount looks like from here.
    let discounted = facturx("R-2026-77").replace(
        "<ram:LineTotalAmount>1000.00</ram:LineTotalAmount>",
        "<ram:LineTotalAmount>900.00</ram:LineTotalAmount>",
    );
    let message = assert_validation(account.import_billing_bill(discounted.as_bytes()).await);
    assert!(message.contains("line 1"), "{message}");
    assert!(message.contains("BT-131"), "{message}");

    // VAT that does not follow from the lines at the rate stated.
    let wrong_vat = facturx("R-2026-77")
        .replace(
            "<ram:TaxTotalAmount currencyID=\"EUR\">231.17</ram:TaxTotalAmount>",
            "<ram:TaxTotalAmount currencyID=\"EUR\">220.16</ram:TaxTotalAmount>",
        )
        .replace(
            "<ram:GrandTotalAmount>1331.97</ram:GrandTotalAmount>",
            "<ram:GrandTotalAmount>1320.96</ram:GrandTotalAmount>",
        )
        .replace(
            "<ram:DuePayableAmount>1331.97</ram:DuePayableAmount>",
            "<ram:DuePayableAmount>1320.96</ram:DuePayableAmount>",
        );
    let message = assert_validation(account.import_billing_bill(wrong_vat.as_bytes()).await);
    assert!(message.contains("BR-CO-14/BR-CO-17"), "{message}");

    // A reverse-charge line: 0 % that means the buyer owes the VAT. Stored as
    // zero-rated it would understate a return, so it is refused by name.
    let reverse_charge = facturx("R-2026-77").replace(
        "<ram:CategoryCode>S</ram:CategoryCode>",
        "<ram:CategoryCode>AE</ram:CategoryCode>",
    );
    let message = assert_validation(account.import_billing_bill(reverse_charge.as_bytes()).await);
    assert!(message.contains("AE"), "{message}");

    // A file that is not an e-invoice at all, and a PDF, which is the obvious
    // thing to try.
    assert_validation(account.import_billing_bill(b"{\"invoice\":true}").await);
    let message = assert_validation(account.import_billing_bill(b"%PDF-1.7 ...").await);
    assert!(message.contains("PDF"), "{message}");

    // Nothing was written by any of them — checked straight from the tables,
    // not through the store's own reads.
    assert!(account.billing_bills(None).await.unwrap().is_empty());
    let bills: i64 = sqlx::query_scalar("SELECT count(*) FROM billing_bills WHERE tenant_id = $1")
        .bind(tenant_id.as_str())
        .fetch_one(&pool)
        .await
        .unwrap();
    let lines: i64 =
        sqlx::query_scalar("SELECT count(*) FROM billing_bill_lines WHERE tenant_id = $1")
            .bind(tenant_id.as_str())
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!((bills, lines), (0, 0), "not a row, not a line");
}

#[tokio::test]
async fn approving_a_bill_is_a_one_way_door_and_deleting_one_is_not_a_way_back() {
    let store = common::test_store().await;
    let (account, _tenant) = tenant(&store, "decide").await;

    let id = account
        .import_billing_bill(facturx("R-2026-77").as_bytes())
        .await
        .unwrap();

    // "received" is not a decision.
    let message = assert_validation(account.decide_billing_bill(&id, BillStatus::Received).await);
    assert!(message.contains("approved or rejected"), "{message}");

    account
        .decide_billing_bill(&id, BillStatus::Approved)
        .await
        .unwrap();
    let approved = account.billing_bill(&id).await.unwrap().unwrap().bill;
    assert_eq!(approved.status, BillStatus::Approved);
    assert!(approved.decided_by.is_some(), "who decided is recorded");
    assert!(approved.decided_at.is_some(), "and when");

    // A second decision, in either direction, is refused.
    for decision in [BillStatus::Approved, BillStatus::Rejected] {
        let message = assert_conflict(account.decide_billing_bill(&id, decision).await);
        assert!(message.contains("already been approved"), "{message}");
    }
    // …and an approved bill is part of the record.
    let message = assert_conflict(account.delete_billing_bill(&id).await);
    assert!(message.contains("cannot be deleted"), "{message}");
    assert!(account.billing_bill(&id).await.unwrap().is_some());

    // A rejection is equally final, and equally undeletable: refusing to pay
    // an invoice is exactly the fact a supplier will later dispute.
    let rejected_id = account
        .import_billing_bill(facturx("R-2026-78").as_bytes())
        .await
        .unwrap();
    account
        .decide_billing_bill(&rejected_id, BillStatus::Rejected)
        .await
        .unwrap();
    assert_conflict(account.delete_billing_bill(&rejected_id).await);
    let listed = account
        .billing_bills(Some(BillStatus::Rejected))
        .await
        .unwrap();
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].id.as_str(), rejected_id.as_str());

    // The undo that does exist: an undecided bill imported by mistake, and its
    // lines with it.
    let mistake = account
        .import_billing_bill(facturx("R-2026-79").as_bytes())
        .await
        .unwrap();
    account.delete_billing_bill(&mistake).await.unwrap();
    assert!(account.billing_bill(&mistake).await.unwrap().is_none());
    // Deleting it releases the number, so the right file can be imported after
    // the wrong one.
    account
        .import_billing_bill(facturx("R-2026-79").as_bytes())
        .await
        .unwrap();
    assert_eq!(account.billing_bills(None).await.unwrap().len(), 3);
}

#[tokio::test]
async fn another_tenant_can_neither_read_decide_nor_delete_a_bill() {
    let store = common::test_store().await;
    let (a, tenant_a) = tenant(&store, "a").await;
    let (b, _tenant_b) = tenant(&store, "b").await;
    let pool = raw_pool().await;

    let a_bill = a
        .import_billing_bill(facturx("R-2026-77").as_bytes())
        .await
        .unwrap();
    let ghost = BillingBillId::new("bill-that-never-existed".to_owned());

    // Reading: the same empty answer for A's id and for an id that never
    // existed — never an existence oracle, never a field of A's.
    assert!(b.billing_bill(&a_bill).await.unwrap().is_none());
    assert!(b.billing_bill(&ghost).await.unwrap().is_none());
    assert!(b.billing_bills(None).await.unwrap().is_empty());
    assert!(
        b.billing_bills(Some(BillStatus::Received))
            .await
            .unwrap()
            .is_empty()
    );

    // Deciding and deleting: `NotFound`, never `Conflict` — a state refusal
    // would confirm both that the id exists and what state it is in.
    assert_not_found(b.decide_billing_bill(&a_bill, BillStatus::Approved).await);
    assert_not_found(b.decide_billing_bill(&a_bill, BillStatus::Rejected).await);
    assert_not_found(b.delete_billing_bill(&a_bill).await);
    assert_not_found(b.decide_billing_bill(&ghost, BillStatus::Approved).await);
    assert_not_found(b.delete_billing_bill(&ghost).await);

    // A's bill is untouched by all of it.
    let after = a.billing_bill(&a_bill).await.unwrap().unwrap();
    assert_eq!(after.bill.status, BillStatus::Received);
    assert!(after.bill.decided_by.is_none());
    assert_eq!(after.lines.len(), 2);

    // The denial is about ownership, not about the operation: B books the same
    // supplier's same document cleanly under their own tenant, and the two
    // bills are separate rows.
    let b_bill = b
        .import_billing_bill(facturx("R-2026-77").as_bytes())
        .await
        .unwrap();
    assert_ne!(b_bill.as_str(), a_bill.as_str());
    b.decide_billing_bill(&b_bill, BillStatus::Approved)
        .await
        .unwrap();
    assert_eq!(
        a.billing_bill(&a_bill).await.unwrap().unwrap().bill.status,
        BillStatus::Received,
        "B's decision is not A's"
    );

    // Read straight from the tables, not through the store's own tenant
    // predicate: A's rows belong to A alone.
    let foreign: i64 =
        sqlx::query_scalar("SELECT count(*) FROM billing_bills WHERE id = $1 AND tenant_id <> $2")
            .bind(a_bill.as_str())
            .bind(tenant_a.as_str())
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(foreign, 0);
    let foreign_lines: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM billing_bill_lines WHERE bill_id = $1 AND tenant_id <> $2",
    )
    .bind(a_bill.as_str())
    .bind(tenant_a.as_str())
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(foreign_lines, 0);

    // Deleting the tenant takes its bills and their lines with it.
    store.delete_tenant(&tenant_a).await.unwrap();
    let left: i64 = sqlx::query_scalar("SELECT count(*) FROM billing_bills WHERE tenant_id = $1")
        .bind(tenant_a.as_str())
        .fetch_one(&pool)
        .await
        .unwrap();
    let left_lines: i64 =
        sqlx::query_scalar("SELECT count(*) FROM billing_bill_lines WHERE tenant_id = $1")
            .bind(tenant_a.as_str())
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!((left, left_lines), (0, 0));
    // B's bill is still there.
    assert!(b.billing_bill(&b_bill).await.unwrap().is_some());
}
