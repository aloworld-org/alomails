//! The `GET /billing/invoices/{id}/xrechnung.xml` route (B1.23), driven through
//! the real router over a real Postgres.
//!
//! The Factur-X route has its own suite (`billing_facturx_http`); what is
//! different about this one, and what this suite is:
//!
//! - **It refuses documents the other route serves.** XRechnung requires terms
//!   EN 16931 leaves optional — the seller's telephone, both post codes, the
//!   buyer's reference — so the same invoice can be a valid Factur-X and an
//!   invalid XRechnung, and the tenant has to be told *which German rule*
//!   (`BR-DE-7`, `BR-DE-15`) and where to fix it.
//! - **A credit note changes schema, not a code.** UBL has two roots, and a 381
//!   inside an `Invoice` element is not a document. That switch is only really
//!   proven end to end, on a credit note the store itself created.
//! - **It is one invoice in two syntaxes.** The figures the UBL file states and
//!   the figures the CII file states are the same money, and the two routes
//!   must never disagree about it.
//!
//! And the mandatory question: can a byte of tenant B's identity reach tenant
//! A's e-invoice. The neighbour below holds a distinctive legal name, telephone
//! number, VAT identifier and bank account, and every assertion looks for them.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use crate::common;

use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use serde_json::{Value, json};
use tower::ServiceExt;

use crate::common::{Harness, harness, send};

// ---- request helpers ---------------------------------------------------------

fn with_json(method: &str, uri: &str, token: Option<&str>, body: Value) -> Request<Body> {
    let mut req = Request::builder()
        .method(method)
        .uri(uri)
        .header("content-type", "application/json");
    if let Some(token) = token {
        req = req.header("authorization", format!("Bearer {token}"));
    }
    req.body(Body::from(body.to_string())).unwrap()
}

async fn post(app: &Router, token: &str, uri: &str, body: Value) -> (StatusCode, Value) {
    send(app, with_json("POST", uri, Some(token), body)).await
}

async fn patch(app: &Router, token: &str, uri: &str, body: Value) -> (StatusCode, Value) {
    send(app, with_json("PATCH", uri, Some(token), body)).await
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

async fn fetch(app: &Router, token: Option<&str>, uri: &str) -> Download {
    let resp = app
        .clone()
        .oneshot(with_json("GET", uri, token, json!({})))
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

async fn xrechnung(h: &Harness, id: &str) -> Download {
    fetch(
        &h.app,
        Some(&h.token),
        &format!("/billing/invoices/{id}/xrechnung.xml"),
    )
    .await
}

fn created_id(kind: &str, (status, body): (StatusCode, Value)) -> String {
    assert_eq!(status, StatusCode::OK, "create failed: {body}");
    body[kind]["id"].as_str().unwrap().to_owned()
}

// ---- fixtures ----------------------------------------------------------------

/// An issuer identity complete enough for XRechnung: everything EN 16931 asks
/// of a seller, plus the contact telephone the German CIUS makes mandatory.
fn identity(name: &str, vat_id: &str, iban: &str, phone: &str) -> Value {
    json!({
        "legalName": name,
        "addressLine1": "Keizersgracht 1",
        "postalCode": "1015 CJ",
        "city": "Amsterdam",
        "country": "NL",
        "vatId": vat_id,
        "registrationNo": "KVK 90123456",
        "email": "billing@alo.test",
        "phone": phone,
        "iban": iban,
        "bic": "ABNANL2A",
        "bankName": "ABN AMRO",
    })
}

async fn a_customer(app: &Router, token: &str, name: &str) -> String {
    created_id(
        "customer",
        post(
            app,
            token,
            "/billing/customers",
            json!({
                "name": name,
                "addressLine1": "Hauptstraße 1",
                "postalCode": "10115",
                "city": "Berlin",
                "country": "DE",
                "vatId": "DE811907980",
                "email": "einkauf@kunde.test",
                "paymentTermsDays": 14,
            }),
        )
        .await,
    )
}

/// A draft with one line, and the customer reference XRechnung routes by.
async fn a_draft(h: &Harness, customer: &str, description: &str, reference: &str) -> String {
    created_id(
        "invoice",
        post(
            &h.app,
            &h.token,
            "/billing/invoices",
            json!({
                "customerId": customer,
                "reference": reference,
                "lines": [{
                    "description": description,
                    "unit": "hour",
                    "qtyMilli": 2_000,
                    "unitPriceCents": 12_000,
                    "vatRateBp": 2_100,
                }],
            }),
        )
        .await,
    )
}

async fn issue(h: &Harness, id: &str) -> String {
    let (status, body) = post(
        &h.app,
        &h.token,
        &format!("/billing/invoices/{id}/issue"),
        json!({}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "issue failed: {body}");
    body["invoice"]["number"].as_str().unwrap().to_owned()
}

/// A tenant that can issue an XRechnung, with one issued invoice.
async fn a_complete_tenant(tag: &str) -> (Harness, String, String) {
    let h = harness(tag).await;
    common::seed_default_chart(&h.acc).await;
    let (status, body) = patch(
        &h.app,
        &h.token,
        "/billing/settings",
        identity(
            "Alo Werkplaats B.V.",
            "NL812345678B01",
            "NL91ABNA0417164300",
            "+31 20 123 4567",
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let customer = a_customer(&h.app, &h.token, "Kunde & Söhne GmbH").await;
    let invoice = a_draft(&h, &customer, "Consulting", "04011000-12345-06").await;
    let number = issue(&h, &invoice).await;
    (h, invoice, number)
}

// ---- guards ------------------------------------------------------------------

#[tokio::test]
async fn the_route_needs_a_token_and_an_id_that_exists() {
    let h = harness("bill-ubl-guards").await;
    common::seed_default_chart(&h.acc).await;

    let anonymous = fetch(&h.app, None, "/billing/invoices/no-such-id/xrechnung.xml").await;
    assert_eq!(anonymous.status, StatusCode::UNAUTHORIZED);
    assert!(!anonymous.text().contains("ubl:Invoice"));

    let missing = xrechnung(&h, "no-such-id").await;
    assert_eq!(missing.status, StatusCode::NOT_FOUND, "{}", missing.text());
}

#[tokio::test]
async fn a_draft_and_a_void_document_have_no_e_invoice() {
    let (h, _, _) = a_complete_tenant("bill-ubl-states").await;
    let customer = a_customer(&h.app, &h.token, "Zweite GmbH").await;

    // A draft has no number and no issue date, so there is nothing to send.
    let draft = a_draft(&h, &customer, "Consulting", "PO-1").await;
    let refused = xrechnung(&h, &draft).await;
    assert_eq!(refused.status, StatusCode::CONFLICT, "{}", refused.text());
    assert!(
        refused.text().contains("issue it first"),
        "{}",
        refused.text()
    );

    // A void document was cancelled; a cancelled e-invoice does not exist.
    let voided = a_draft(&h, &customer, "Cancelled work", "PO-2").await;
    issue(&h, &voided).await;
    let (status, body) = post(
        &h.app,
        &h.token,
        &format!("/billing/invoices/{voided}/void"),
        json!({}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let refused = xrechnung(&h, &voided).await;
    assert_eq!(refused.status, StatusCode::CONFLICT, "{}", refused.text());
    assert!(refused.text().contains("credit note"), "{}", refused.text());
}

#[tokio::test]
async fn the_german_rules_refuse_what_the_european_ones_allow() {
    // A tenant with a complete EN 16931 identity and no telephone number, and
    // an invoice with no customer reference: a valid Factur-X, and not an
    // XRechnung.
    let h = harness("bill-ubl-de-rules").await;
    common::seed_default_chart(&h.acc).await;
    let (status, body) = patch(
        &h.app,
        &h.token,
        "/billing/settings",
        identity(
            "Alo Werkplaats B.V.",
            "NL812345678B01",
            "NL91ABNA0417164300",
            "",
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let customer = a_customer(&h.app, &h.token, "Kunde GmbH").await;
    let invoice = a_draft(&h, &customer, "Consulting", "").await;
    issue(&h, &invoice).await;

    // Factur-X is served: the European standard asks for neither term.
    let facturx = fetch(
        &h.app,
        Some(&h.token),
        &format!("/billing/invoices/{invoice}/facturx.xml"),
    )
    .await;
    assert_eq!(facturx.status, StatusCode::OK, "{}", facturx.text());

    // XRechnung is refused, by identifier, with somewhere to go.
    let refused = xrechnung(&h, &invoice).await;
    assert_eq!(
        refused.status,
        StatusCode::UNPROCESSABLE_ENTITY,
        "{}",
        refused.text()
    );
    let detail = refused.text();
    for rule in ["BR-DE-7", "BR-DE-15"] {
        assert!(detail.contains(rule), "{rule} is not reported: {detail}");
    }
    assert!(detail.contains("Leitweg-ID"), "{detail}");
    assert!(detail.contains("billing details"), "{detail}");
    assert!(detail.contains("XRechnung"), "{detail}");

    // The seller's details are read live, so filling in the telephone fixes
    // every document at once — including this one, which now breaks one rule.
    let (status, body) = patch(
        &h.app,
        &h.token,
        "/billing/settings",
        json!({"phone": "+31 20 123 4567"}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let still = xrechnung(&h, &invoice).await;
    assert_eq!(still.status, StatusCode::UNPROCESSABLE_ENTITY);
    assert!(still.text().contains("BR-DE-15"), "{}", still.text());
    assert!(!still.text().contains("BR-DE-7"), "{}", still.text());

    // The reference, though, belongs to the frozen document: an issued invoice
    // cannot be edited into compliance, which is the whole point of issuing.
    let (status, _) = patch(
        &h.app,
        &h.token,
        &format!("/billing/invoices/{invoice}"),
        json!({"reference": "04011000-12345-06"}),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);

    // So the fix is a document raised with one, and that one is served.
    let second = a_draft(&h, &customer, "Consulting", "04011000-12345-06").await;
    issue(&h, &second).await;
    let served = xrechnung(&h, &second).await;
    assert_eq!(served.status, StatusCode::OK, "{}", served.text());
    assert!(
        served
            .text()
            .contains("<cbc:BuyerReference>04011000-12345-06</cbc:BuyerReference>")
    );
}

#[tokio::test]
async fn an_issuer_who_has_not_stated_its_own_details_is_told_both_rule_sets_at_once() {
    // A tenant that has never opened the billing settings gets one answer
    // naming every rule it breaks, European and German, so the details are
    // filled in once rather than twice.
    let h = harness("bill-ubl-incomplete").await;
    common::seed_default_chart(&h.acc).await;
    let customer = a_customer(&h.app, &h.token, "Kunde GmbH").await;
    let invoice = a_draft(&h, &customer, "Consulting", "PO-9").await;
    issue(&h, &invoice).await;

    let refused = xrechnung(&h, &invoice).await;
    assert_eq!(
        refused.status,
        StatusCode::UNPROCESSABLE_ENTITY,
        "{}",
        refused.text()
    );
    let detail = refused.text();
    for rule in [
        "BR-06", "BR-08", "BR-09", "BR-CO-26", "BR-S-02", "BR-DE-3", "BR-DE-4", "BR-DE-5",
        "BR-DE-6", "BR-DE-7", "BR-DE-8", "BR-DE-16",
    ] {
        assert!(detail.contains(rule), "{rule} is not reported: {detail}");
    }
}

// ---- the document itself -----------------------------------------------------

#[tokio::test]
async fn an_issued_invoice_downloads_as_an_xrechnung_document() {
    let (h, invoice, number) = a_complete_tenant("bill-ubl-issued").await;
    let file = xrechnung(&h, &invoice).await;
    assert_eq!(file.status, StatusCode::OK, "{}", file.text());

    // The response's own contract: a file, never sniffed, never cached.
    assert_eq!(
        file.header("content-type"),
        "application/xml; charset=utf-8"
    );
    assert_eq!(
        file.header("content-disposition"),
        format!("attachment; filename=\"Invoice-{number}-xrechnung.xml\"")
    );
    assert_eq!(file.header("x-content-type-options"), "nosniff");
    assert_eq!(file.header("cache-control"), "no-store");

    let xml = file.text();
    assert!(xml.contains("<ubl:Invoice "));
    assert!(xml.contains(
        "<cbc:CustomizationID>urn:cen.eu:en16931:2017#compliant#urn:xoev-de:kosit:standard:xrechnung_3.0</cbc:CustomizationID>"
    ));
    assert!(xml.contains(&format!("<cbc:ID>{number}</cbc:ID>")));
    assert!(xml.contains("<cbc:InvoiceTypeCode>380</cbc:InvoiceTypeCode>"));
    // The store's figures: 2 h at 120.00 is 240.00 net, 21 % is 50.40.
    assert!(
        xml.contains(
            "<cbc:LineExtensionAmount currencyID=\"EUR\">240.00</cbc:LineExtensionAmount>"
        )
    );
    assert!(xml.contains("<cbc:TaxAmount currencyID=\"EUR\">50.40</cbc:TaxAmount>"));
    assert!(
        xml.contains("<cbc:TaxInclusiveAmount currencyID=\"EUR\">290.40</cbc:TaxInclusiveAmount>")
    );
    assert!(xml.contains("<cbc:PayableAmount currencyID=\"EUR\">290.40</cbc:PayableAmount>"));
    // Both parties, both VAT identifiers, the seller's contact desk, the bank.
    assert!(xml.contains("<cbc:CompanyID>NL812345678B01</cbc:CompanyID>"));
    assert!(xml.contains("<cbc:CompanyID>DE811907980</cbc:CompanyID>"));
    assert!(xml.contains("<cbc:Telephone>+31 20 123 4567</cbc:Telephone>"));
    assert!(xml.contains("<cbc:ID>NL91ABNA0417164300</cbc:ID>"));
}

#[tokio::test]
async fn one_invoice_states_the_same_money_in_both_syntaxes() {
    let (h, invoice, number) = a_complete_tenant("bill-ubl-both").await;
    let ubl = xrechnung(&h, &invoice).await.text();
    let cii = fetch(
        &h.app,
        Some(&h.token),
        &format!("/billing/invoices/{invoice}/facturx.xml"),
    )
    .await
    .text();

    // Two files that share almost no bytes and no figure at all.
    assert!(ubl.contains("<ubl:Invoice "));
    assert!(cii.contains("<rsm:CrossIndustryInvoice "));
    assert!(ubl.contains(&format!("<cbc:ID>{number}</cbc:ID>")));
    assert!(cii.contains(&format!("<ram:ID>{number}</ram:ID>")));
    for figure in ["240.00", "50.40", "290.40"] {
        assert!(ubl.contains(figure), "the UBL file is missing {figure}");
        assert!(cii.contains(figure), "the CII file is missing {figure}");
    }
    // The same day, spelled the way each syntax spells a date.
    assert!(ubl.contains("<cbc:IssueDate>20"));
    assert!(cii.contains("format=\"102\""));
}

#[tokio::test]
async fn a_credit_note_is_a_credit_note_document_and_not_an_invoice_with_a_code() {
    let (h, invoice, number) = a_complete_tenant("bill-ubl-credit").await;
    let (status, body) = post(
        &h.app,
        &h.token,
        &format!("/billing/invoices/{invoice}/credit-note"),
        json!({}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let credit = body["invoice"]["id"].as_str().unwrap().to_owned();
    issue(&h, &credit).await;

    let xml = xrechnung(&h, &credit).await.text();
    assert!(xml.contains("<ubl:CreditNote "));
    assert!(xml.contains("urn:oasis:names:specification:ubl:schema:xsd:CreditNote-2"));
    assert!(xml.contains("<cbc:CreditNoteTypeCode>381</cbc:CreditNoteTypeCode>"));
    assert!(xml.contains("<cac:CreditNoteLine>"));
    assert!(xml.contains("<cbc:CreditedQuantity"));
    assert!(!xml.contains("<cac:InvoiceLine>"));
    assert!(!xml.contains("InvoiceTypeCode"));
    // It names what it corrects, and asks for nothing.
    assert!(xml.contains(&format!("<cbc:ID>{number}</cbc:ID>")));
    assert!(xml.contains("<cbc:PaymentMeansCode>1</cbc:PaymentMeansCode>"));
    assert!(!xml.contains("PayeeFinancialAccount"));
    assert!(!xml.contains("<cbc:DueDate>"));
    // Positive on the wire, negative in our ledger.
    assert!(xml.contains("<cbc:PayableAmount currencyID=\"EUR\">290.40</cbc:PayableAmount>"));
    assert!(
        !xml.contains(">-"),
        "a credit note carries no negative: {xml}"
    );
}

// ---- the mandatory wrong-tenant proof ----------------------------------------

#[tokio::test]
async fn no_byte_of_another_tenants_document_reaches_an_e_invoice() {
    let (a, a_invoice, _) = a_complete_tenant("bill-ubl-a").await;
    let b = harness("bill-ubl-b").await;
    common::seed_default_chart(&b.acc).await;

    // B states an identity nobody else could plausibly have.
    let (status, body) = patch(
        &b.app,
        &b.token,
        "/billing/settings",
        identity(
            "NACHBAR-SECRET GmbH",
            "NL999888778B01",
            "NL76INGB0006174254",
            "+49 30 999 8887",
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let b_customer = a_customer(&b.app, &b.token, "B-SECRET-CUSTOMER").await;
    let b_invoice = a_draft(&b, &b_customer, "B-SECRET-WORK", "B-SECRET-REF").await;
    issue(&b, &b_invoice).await;

    // B's document is a 404 to A — not a 403 and not a 422, either of which
    // would confirm the id exists — and the refusal leaks nothing.
    let refusal = xrechnung(&a, &b_invoice).await;
    assert_eq!(refusal.status, StatusCode::NOT_FOUND);
    let refused = refusal.text();
    for secret in [
        "NACHBAR-SECRET",
        "NL999888778B01",
        "NL76INGB0006174254",
        "+49 30 999 8887",
        "B-SECRET-CUSTOMER",
        "B-SECRET-WORK",
        "B-SECRET-REF",
    ] {
        assert!(!refused.contains(secret), "{secret} leaked in: {refused}");
    }
    // An id that never existed anywhere gets the identical answer.
    let ghost = xrechnung(&a, "ghost").await;
    assert_eq!(ghost.status, StatusCode::NOT_FOUND);
    assert_eq!(ghost.text(), refused);

    // And A's own e-invoice carries nothing of B's, though B's rows are in the
    // same tables.
    let a_xml = xrechnung(&a, &a_invoice).await.text();
    for secret in [
        "NACHBAR",
        "NL999888778B01",
        "INGB",
        "+49 30",
        "B-SECRET-CUSTOMER",
        "B-SECRET-WORK",
        "B-SECRET-REF",
    ] {
        assert!(
            !a_xml.contains(secret),
            "{secret} reached the neighbour's e-invoice"
        );
    }
    assert!(a_xml.contains("Kunde &amp; S\u{f6}hne GmbH"));
}
