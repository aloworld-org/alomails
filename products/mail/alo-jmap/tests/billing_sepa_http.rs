//! Paying suppliers over HTTP (B2.12): `POST /billing/bills/sepa.xml`, driven
//! through the real router over a real Postgres.
//!
//! The arc this suite walks is the whole feature: a supplier's e-invoice is
//! uploaded, approved, and paid — and the file that comes back is checked
//! against the schema subset and the scheme's rules rather than merely being
//! non-empty.
//!
//! Four things are specific to this route and are what the suite is for:
//!
//! - **A `POST` that answers with a file.** So the headers matter as much as
//!   the body: an attachment, `nosniff`, `no-store`, named after the run.
//! - **It records what it hands over.** The bills it covered come back marked,
//!   and the second run over one of them is a `409` — the wire proof of "a
//!   supplier is not paid twice by accident".
//! - **Nothing is recorded when nothing is handed over.** Every refusal leaves
//!   the bills exactly as they were.
//! - **The neighbour's bills.** Tenant B naming A's bill id gets the same `404`
//!   as an id that never existed, and A's bill is untouched afterwards.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use serde_json::{Value, json};
use tower::ServiceExt;

use crate::common::{Harness, harness, send};
use alo_jmap::billing_pain001::Pain001Version;
use alo_jmap::billing_pain001_rules::violations;

// ---- request helpers ---------------------------------------------------------

fn with_body(method: &str, uri: &str, token: Option<&str>, body: &Value) -> Request<Body> {
    let mut req = Request::builder()
        .method(method)
        .uri(uri)
        .header("content-type", "application/json");
    if let Some(token) = token {
        req = req.header("authorization", format!("Bearer {token}"));
    }
    req.body(Body::from(body.to_string())).unwrap()
}

/// A downloaded file: the status, the headers that matter, and the bytes.
struct Download {
    status: StatusCode,
    headers: Vec<(String, String)>,
    bytes: Vec<u8>,
}

impl Download {
    fn header(&self, name: &str) -> String {
        self.headers
            .iter()
            .find(|(k, _)| k == name)
            .map_or_else(String::new, |(_, v)| v.clone())
    }

    fn text(&self) -> String {
        String::from_utf8_lossy(&self.bytes).into_owned()
    }
}

/// The payment run, as a raw response: the body is a file, not JSON.
async fn run(app: &Router, token: Option<&str>, body: &Value) -> Download {
    let resp = app
        .clone()
        .oneshot(with_body("POST", "/billing/bills/sepa.xml", token, body))
        .await
        .unwrap();
    let status = resp.status();
    let headers = resp
        .headers()
        .iter()
        .map(|(k, v)| {
            (
                k.as_str().to_owned(),
                v.to_str().unwrap_or_default().to_owned(),
            )
        })
        .collect();
    let bytes = axum::body::to_bytes(resp.into_body(), 8 * 1024 * 1024)
        .await
        .unwrap()
        .to_vec();
    Download {
        status,
        headers,
        bytes,
    }
}

/// A run that is expected to be refused, answering with the `Problem` JSON.
async fn refused(app: &Router, token: &str, body: &Value) -> (StatusCode, Value) {
    send(
        app,
        with_body("POST", "/billing/bills/sepa.xml", Some(token), body),
    )
    .await
}

async fn get(app: &Router, token: &str, uri: &str) -> (StatusCode, Value) {
    let req = Request::builder()
        .method("GET")
        .uri(uri)
        .header("authorization", format!("Bearer {token}"))
        .body(Body::empty())
        .unwrap();
    send(app, req).await
}

// ---- fixtures ----------------------------------------------------------------

/// A supplier's Factur-X invoice, stating the account they want paying into.
///
/// €1331.97 payable, hand-computed: 8 h at €125.00 plus 240 km at €0.42 is
/// €1100.80 net, 21 % of which rounds once at the rate subtotal to €231.17.
fn supplier_invoice(number: &str, iban: &str) -> String {
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<rsm:CrossIndustryInvoice xmlns:rsm="urn:un:unece:uncefact:data:standard:CrossIndustryInvoice:100" xmlns:ram="urn:un:unece:uncefact:data:standard:ReusableAggregateBusinessInformationEntity:100" xmlns:udt="urn:un:unece:uncefact:data:standard:UnqualifiedDataType:100">
  <rsm:ExchangedDocument>
    <ram:ID>{number}</ram:ID>
    <ram:TypeCode>380</ram:TypeCode>
    <ram:IssueDateTime><udt:DateTimeString format="102">20260807</udt:DateTimeString></ram:IssueDateTime>
  </rsm:ExchangedDocument>
  <rsm:SupplyChainTradeTransaction>
    <ram:IncludedSupplyChainTradeLineItem>
      <ram:SpecifiedTradeProduct><ram:Name>Beratung</ram:Name></ram:SpecifiedTradeProduct>
      <ram:SpecifiedLineTradeAgreement><ram:NetPriceProductTradePrice><ram:ChargeAmount>125.00</ram:ChargeAmount></ram:NetPriceProductTradePrice></ram:SpecifiedLineTradeAgreement>
      <ram:SpecifiedLineTradeDelivery><ram:BilledQuantity unitCode="HUR">8</ram:BilledQuantity></ram:SpecifiedLineTradeDelivery>
      <ram:SpecifiedLineTradeSettlement>
        <ram:ApplicableTradeTax><ram:CategoryCode>S</ram:CategoryCode><ram:RateApplicablePercent>21.00</ram:RateApplicablePercent></ram:ApplicableTradeTax>
        <ram:SpecifiedTradeSettlementLineMonetarySummation><ram:LineTotalAmount>1000.00</ram:LineTotalAmount></ram:SpecifiedTradeSettlementLineMonetarySummation>
      </ram:SpecifiedLineTradeSettlement>
    </ram:IncludedSupplyChainTradeLineItem>
    <ram:IncludedSupplyChainTradeLineItem>
      <ram:SpecifiedTradeProduct><ram:Name>Fahrtkosten</ram:Name></ram:SpecifiedTradeProduct>
      <ram:SpecifiedLineTradeAgreement><ram:NetPriceProductTradePrice><ram:ChargeAmount>0.42</ram:ChargeAmount></ram:NetPriceProductTradePrice></ram:SpecifiedLineTradeAgreement>
      <ram:SpecifiedLineTradeDelivery><ram:BilledQuantity unitCode="KMT">240</ram:BilledQuantity></ram:SpecifiedLineTradeDelivery>
      <ram:SpecifiedLineTradeSettlement>
        <ram:ApplicableTradeTax><ram:CategoryCode>S</ram:CategoryCode><ram:RateApplicablePercent>21.00</ram:RateApplicablePercent></ram:ApplicableTradeTax>
        <ram:SpecifiedTradeSettlementLineMonetarySummation><ram:LineTotalAmount>100.80</ram:LineTotalAmount></ram:SpecifiedTradeSettlementLineMonetarySummation>
      </ram:SpecifiedLineTradeSettlement>
    </ram:IncludedSupplyChainTradeLineItem>
    <ram:ApplicableHeaderTradeAgreement>
      <ram:SellerTradeParty>
        <ram:Name>Müller &amp; Söhne GmbH</ram:Name>
        <ram:PostalTradeAddress><ram:PostcodeCode>10115</ram:PostcodeCode><ram:LineOne>Hauptstraße 5</ram:LineOne><ram:CityName>Berlin</ram:CityName><ram:CountryID>DE</ram:CountryID></ram:PostalTradeAddress>
        <ram:SpecifiedTaxRegistration><ram:ID schemeID="VA">DE811907980</ram:ID></ram:SpecifiedTaxRegistration>
      </ram:SellerTradeParty>
    </ram:ApplicableHeaderTradeAgreement>
    <ram:ApplicableHeaderTradeSettlement>
      <ram:InvoiceCurrencyCode>EUR</ram:InvoiceCurrencyCode>
      <ram:PaymentReference>{number}</ram:PaymentReference>
      <ram:SpecifiedTradeSettlementPaymentMeans>
        <ram:TypeCode>30</ram:TypeCode>
        <ram:PayeePartyCreditorFinancialAccount><ram:IBANID>{iban}</ram:IBANID></ram:PayeePartyCreditorFinancialAccount>
      </ram:SpecifiedTradeSettlementPaymentMeans>
      <ram:ApplicableTradeTax><ram:CalculatedAmount>231.17</ram:CalculatedAmount><ram:BasisAmount>1100.80</ram:BasisAmount><ram:CategoryCode>S</ram:CategoryCode><ram:RateApplicablePercent>21.00</ram:RateApplicablePercent></ram:ApplicableTradeTax>
      <ram:SpecifiedTradePaymentTerms><ram:DueDateDateTime><udt:DateTimeString format="102">20260906</udt:DateTimeString></ram:DueDateDateTime></ram:SpecifiedTradePaymentTerms>
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

/// States the tenant's own identity and bank account — without which no payment
/// file can be produced at all.
async fn state_our_account(h: &Harness) {
    let (status, body) = send(
        &h.app,
        with_body(
            "PATCH",
            "/billing/settings",
            Some(&h.token),
            &json!({
                "legalName": "Alo Werkplaats B.V.",
                "addressLine1": "Keizersgracht 1",
                "postalCode": "1015 CJ",
                "city": "Amsterdam",
                "country": "NL",
                "email": "billing@alo.test",
                "iban": "NL91 ABNA 0417 1643 00",
                "bic": "ABNANL2A",
            }),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "settings: {body}");
}

/// Imports a supplier's invoice and approves it, answering with the bill id.
async fn approved_bill(h: &Harness, number: &str, iban: &str) -> String {
    let file = supplier_invoice(number, iban);
    let req = Request::builder()
        .method("POST")
        .uri("/billing/bills/import")
        .header("content-type", "application/xml")
        .header("authorization", format!("Bearer {}", h.token))
        .body(Body::from(file))
        .unwrap();
    let (status, body) = send(&h.app, req).await;
    assert_eq!(status, StatusCode::OK, "import: {body}");
    let id = body["bill"]["id"].as_str().unwrap().to_owned();
    let req = Request::builder()
        .method("POST")
        .uri(format!("/billing/bills/{id}/approve"))
        .header("authorization", format!("Bearer {}", h.token))
        .body(Body::empty())
        .unwrap();
    let (status, body) = send(&h.app, req).await;
    assert_eq!(status, StatusCode::OK, "approve: {body}");
    id
}

// ---- the arc -----------------------------------------------------------------

#[tokio::test]
async fn the_arc_from_an_approved_bill_to_a_file_the_bank_can_execute() {
    let h = harness("sepa-arc").await;
    state_our_account(&h).await;

    // No token, no payment file.
    let anonymous = run(&h.app, None, &json!({"billIds": ["x"]})).await;
    assert_eq!(anonymous.status, StatusCode::UNAUTHORIZED);

    let first = approved_bill(&h, "R-2026-77", "DE89370400440532013000").await;
    let second = approved_bill(&h, "R-2026-78", "PL61109010140000071219812874").await;

    // What is waiting to be paid, before anything is instructed.
    let (status, body) = get(&h.app, &h.token, "/billing/bills?payable=true").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["bills"].as_array().map(Vec::len), Some(2));
    assert_eq!(body["bills"][0]["exportedAt"], json!(null));

    // ---- the run ---------------------------------------------------------
    let file = run(
        &h.app,
        Some(&h.token),
        &json!({"billIds": [first, second], "executionDate": "2026-12-31"}),
    )
    .await;
    assert_eq!(file.status, StatusCode::OK, "{}", file.text());
    // The three headers a file is served with, and the name of the run.
    assert_eq!(
        file.header("content-type"),
        "application/xml; charset=utf-8"
    );
    assert!(
        file.header("content-disposition")
            .starts_with("attachment; filename=\"sepa-credit-transfer-ALO"),
        "{}",
        file.header("content-disposition")
    );
    assert_eq!(file.header("x-content-type-options"), "nosniff");
    assert_eq!(file.header("cache-control"), "no-store");

    // The file itself is one our reading of the standard accepts…
    let xml = file.text();
    let broken = violations(&xml, Pain001Version::V03);
    assert!(broken.is_empty(), "{broken:?}");
    // …and says what it was asked to say. €1331.97 twice.
    assert!(xml.contains("<CtrlSum>2663.94</CtrlSum>"), "{xml}");
    assert!(xml.contains("<NbOfTxs>2</NbOfTxs>"));
    assert!(xml.contains("<ReqdExctnDt>2026-12-31</ReqdExctnDt>"));
    assert!(xml.contains("<IBAN>DE89370400440532013000</IBAN>"));
    assert!(xml.contains("<IBAN>PL61109010140000071219812874</IBAN>"));
    // The supplier's name reaches the bank in a spelling it can carry.
    assert!(xml.contains("<Nm>Muller + Sohne GmbH</Nm>"), "{xml}");
    // And our own account is the one the money leaves.
    assert!(xml.contains("<IBAN>NL91ABNA0417164300</IBAN>"));
    assert!(xml.contains("<BIC>ABNANL2A</BIC>"));

    // ---- what the run recorded -------------------------------------------
    let (_, body) = get(&h.app, &h.token, &format!("/billing/bills/{first}")).await;
    let run_id = body["bill"]["exportMessageId"].as_str().unwrap().to_owned();
    assert!(xml.contains(&format!("<MsgId>{run_id}</MsgId>")));
    assert!(body["bill"]["exportedAt"].is_string());
    assert!(body["bill"]["exportedBy"].is_string());
    // Not a payment: the bill is still simply approved.
    assert_eq!(body["bill"]["status"], "approved");

    // Nothing is left waiting.
    let (_, body) = get(&h.app, &h.token, "/billing/bills?payable=true").await;
    assert_eq!(body["bills"].as_array().map(Vec::len), Some(0));

    // ---- and it is not paid twice ----------------------------------------
    let (status, problem) = refused(&h.app, &h.token, &json!({"billIds": [first]})).await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert!(
        problem["detail"]
            .as_str()
            .unwrap_or_default()
            .contains("already in a payment file"),
        "{problem}"
    );

    // …unless the repeat is deliberate, which is a different run.
    let again = run(
        &h.app,
        Some(&h.token),
        &json!({"billIds": [first], "repeat": true}),
    )
    .await;
    assert_eq!(again.status, StatusCode::OK, "{}", again.text());
    assert!(!again.text().contains(&format!("<MsgId>{run_id}</MsgId>")));
}

#[tokio::test]
async fn the_bank_gets_the_version_it_asked_for_and_nothing_else() {
    let h = harness("sepa-version").await;
    state_our_account(&h).await;
    let id = approved_bill(&h, "R-2026-77", "DE89370400440532013000").await;

    let file = run(
        &h.app,
        Some(&h.token),
        &json!({"billIds": [id], "version": "pain.001.001.09"}),
    )
    .await;
    assert_eq!(file.status, StatusCode::OK, "{}", file.text());
    let xml = file.text();
    assert!(violations(&xml, Pain001Version::V09).is_empty());
    assert!(xml.contains("urn:iso:std:iso:20022:tech:xsd:pain.001.001.09"));
    assert!(xml.contains("<BICFI>ABNANL2A</BICFI>"));

    // A version we do not write is a 422 naming the two we do.
    let (status, problem) = refused(
        &h.app,
        &h.token,
        &json!({"billIds": [id], "version": "pain.001.001.11"}),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert!(
        problem["detail"]
            .as_str()
            .unwrap_or_default()
            .contains("pain.001.001.03"),
        "{problem}"
    );
}

#[tokio::test]
async fn a_refused_run_records_nothing() {
    let h = harness("sepa-refused").await;
    state_our_account(&h).await;
    let approved = approved_bill(&h, "R-2026-77", "DE89370400440532013000").await;

    // An undecided bill in the selection stops the whole run — including the
    // approved bill it was selected with, which stays payable.
    let file = supplier_invoice("R-2026-78", "DE89370400440532013000");
    let req = Request::builder()
        .method("POST")
        .uri("/billing/bills/import")
        .header("content-type", "application/xml")
        .header("authorization", format!("Bearer {}", h.token))
        .body(Body::from(file))
        .unwrap();
    let (_, body) = send(&h.app, req).await;
    let undecided = body["bill"]["id"].as_str().unwrap().to_owned();

    for (body, status) in [
        (
            json!({"billIds": [&approved, &undecided]}),
            StatusCode::CONFLICT,
        ),
        (json!({"billIds": []}), StatusCode::UNPROCESSABLE_ENTITY),
        (
            json!({"billIds": [&approved], "executionDate": "yesterday"}),
            StatusCode::UNPROCESSABLE_ENTITY,
        ),
        (
            json!({"billIds": [&approved], "executionDate": "2020-01-01"}),
            StatusCode::UNPROCESSABLE_ENTITY,
        ),
        (json!({"billIds": ["never-existed"]}), StatusCode::NOT_FOUND),
    ] {
        let (got, problem) = refused(&h.app, &h.token, &body).await;
        assert_eq!(got, status, "{body} → {problem}");
    }
    // A body that is not JSON at all is the plain 400 every billing route gives.
    let bad = run(&h.app, Some(&h.token), &json!("not an object")).await;
    assert_eq!(bad.status, StatusCode::BAD_REQUEST);

    // Every one of those left the bill exactly where it was.
    let (_, body) = get(&h.app, &h.token, "/billing/bills?payable=true").await;
    assert_eq!(body["bills"].as_array().map(Vec::len), Some(1));
    assert_eq!(body["bills"][0]["id"], approved);
    assert_eq!(body["bills"][0]["exportedAt"], json!(null));
}

#[tokio::test]
async fn a_tenant_without_a_stated_account_is_told_which_field_is_missing() {
    let h = harness("sepa-blank").await;
    let id = approved_bill(&h, "R-2026-77", "DE89370400440532013000").await;
    let (status, problem) = refused(&h.app, &h.token, &json!({"billIds": [id]})).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert!(
        problem["detail"]
            .as_str()
            .unwrap_or_default()
            .contains("your own"),
        "{problem}"
    );
}

#[tokio::test]
async fn one_tenants_bills_are_never_in_another_tenants_payment_run() {
    let a = harness("sepa-a").await;
    let b = harness("sepa-b").await;
    state_our_account(&a).await;
    state_our_account(&b).await;

    let mine = approved_bill(&a, "R-2026-77", "DE89370400440532013000").await;
    let theirs = approved_bill(&b, "R-2026-90", "DE89370400440532013000").await;

    // B naming A's bill gets the same answer as an id that never existed.
    let (status, first) = refused(&b.app, &b.token, &json!({"billIds": [&mine]})).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let (status, second) = refused(&b.app, &b.token, &json!({"billIds": ["never-existed"]})).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(first, second, "the two refusals differ by not one byte");
    // Nor mixed in with one of B's own, which is how a guessed id arrives.
    let (status, _) = refused(&b.app, &b.token, &json!({"billIds": [&theirs, &mine]})).await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    // B's own payable list never mentions A's bill…
    let (_, body) = get(&b.app, &b.token, "/billing/bills?payable=true").await;
    assert_eq!(body["bills"].as_array().map(Vec::len), Some(1));
    assert_eq!(body["bills"][0]["id"], theirs);
    // …and A's bill is untouched by any of it, then pays normally.
    let (_, body) = get(&a.app, &a.token, &format!("/billing/bills/{mine}")).await;
    assert_eq!(body["bill"]["exportedAt"], json!(null));
    let file = run(&a.app, Some(&a.token), &json!({"billIds": [&mine]})).await;
    assert_eq!(file.status, StatusCode::OK, "{}", file.text());
}
