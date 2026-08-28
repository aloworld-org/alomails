//! The `GET /billing/invoices/{id}/facturx.xml` route and the hybrid PDF that
//! carries the same document (B1.22), driven through the real router over a
//! real Postgres.
//!
//! Three things are different about an e-invoice from every other billing
//! response, and this suite is those three:
//!
//! - **It is read by a machine that will refuse it.** So the refusals are the
//!   interesting half: a draft has no e-invoice, a void document has none, and
//!   an issuer whose own details are incomplete is told the EN 16931 rule
//!   identifier (`BR-09`) rather than being handed XML a customer's gateway
//!   will reject next week.
//! - **It travels twice.** The XML served on its own and the XML inside the
//!   PDF must be the same bytes, or an archive contains two versions of one
//!   invoice.
//! - **It leaves our origin as a file**, so the response's own contract —
//!   attachment, `nosniff`, `no-store` — is part of the route.
//!
//! And the mandatory question: can a byte of tenant B's identity reach tenant
//! A's e-invoice. The neighbour below holds a distinctive legal name, VAT
//! identifier and bank account, and every assertion looks for them.

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

fn created_id(kind: &str, (status, body): (StatusCode, Value)) -> String {
    assert_eq!(status, StatusCode::OK, "create failed: {body}");
    body[kind]["id"].as_str().unwrap().to_owned()
}

/// The `factur-x.xml` a PDF carries, as text — extracted the way a receiving
/// system does it: find the file specification, then read the stream it names.
///
/// Deliberately naive about PDF structure, and that is the point: if a
/// bookkeeping system can find the XML with a byte search for the opening tag,
/// so can this test, and if it cannot, neither can the system.
fn embedded_xml(pdf: &[u8]) -> String {
    let text = String::from_utf8_lossy(pdf);
    let start = text
        .find("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<rsm:CrossIndustryInvoice")
        .unwrap_or_else(|| panic!("no e-invoice inside the PDF"));
    let end = text
        .find("</rsm:CrossIndustryInvoice>")
        .map(|at| at + "</rsm:CrossIndustryInvoice>".len() + 1)
        .unwrap_or(text.len());
    text[start..end].to_owned()
}

// ---- fixtures ----------------------------------------------------------------

/// A complete issuer identity — everything EN 16931 asks of a seller.
fn identity(name: &str, vat_id: &str, iban: &str) -> Value {
    json!({
        "legalName": name,
        "addressLine1": "Keizersgracht 1",
        "postalCode": "1015 CJ",
        "city": "Amsterdam",
        "country": "NL",
        "vatId": vat_id,
        "registrationNo": "KVK 90123456",
        "email": "billing@alo.test",
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

async fn a_draft(h: &Harness, customer: &str, description: &str) -> String {
    created_id(
        "invoice",
        post(
            &h.app,
            &h.token,
            "/billing/invoices",
            json!({
                "customerId": customer,
                "reference": "PO-42",
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

/// A tenant that has stated its identity, with one issued invoice.
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
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let customer = a_customer(&h.app, &h.token, "Kunde & Söhne GmbH").await;
    let invoice = a_draft(&h, &customer, "Consulting").await;
    let number = issue(&h, &invoice).await;
    (h, invoice, number)
}

// ---- guards ------------------------------------------------------------------

#[tokio::test]
async fn the_route_needs_a_token_and_an_id_that_exists() {
    let h = harness("bill-fx-guards").await;
    common::seed_default_chart(&h.acc).await;

    let anonymous = fetch(&h.app, None, "/billing/invoices/no-such-id/facturx.xml").await;
    assert_eq!(anonymous.status, StatusCode::UNAUTHORIZED);
    assert!(!anonymous.text().contains("CrossIndustryInvoice"));

    let missing = fetch(
        &h.app,
        Some(&h.token),
        "/billing/invoices/no-such-id/facturx.xml",
    )
    .await;
    assert_eq!(missing.status, StatusCode::NOT_FOUND, "{}", missing.text());
}

#[tokio::test]
async fn a_draft_and_a_void_document_have_no_e_invoice() {
    let (h, _, _) = a_complete_tenant("bill-fx-states").await;
    let customer = a_customer(&h.app, &h.token, "Zweite GmbH").await;

    // A draft has no number and no issue date, so there is nothing to send.
    let draft = a_draft(&h, &customer, "Consulting").await;
    let refused = fetch(
        &h.app,
        Some(&h.token),
        &format!("/billing/invoices/{draft}/facturx.xml"),
    )
    .await;
    assert_eq!(refused.status, StatusCode::CONFLICT, "{}", refused.text());
    assert!(
        refused.text().contains("issue it first"),
        "{}",
        refused.text()
    );

    // A void document was cancelled; a cancelled e-invoice does not exist.
    let voided = a_draft(&h, &customer, "Cancelled work").await;
    issue(&h, &voided).await;
    let (status, body) = post(
        &h.app,
        &h.token,
        &format!("/billing/invoices/{voided}/void"),
        json!({}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let refused = fetch(
        &h.app,
        Some(&h.token),
        &format!("/billing/invoices/{voided}/facturx.xml"),
    )
    .await;
    assert_eq!(refused.status, StatusCode::CONFLICT, "{}", refused.text());
    assert!(refused.text().contains("credit note"), "{}", refused.text());

    // Both still print: a document that cannot be an e-invoice is still paper.
    for id in [&draft, &voided] {
        let pdf = fetch(
            &h.app,
            Some(&h.token),
            &format!("/billing/invoices/{id}/pdf"),
        )
        .await;
        assert_eq!(pdf.status, StatusCode::OK);
        assert!(pdf.bytes.starts_with(b"%PDF-1.7"));
        assert!(
            !pdf.text().contains("CrossIndustryInvoice"),
            "a document with no e-invoice must not carry one"
        );
    }
}

#[tokio::test]
async fn an_issuer_who_has_not_stated_its_own_details_is_told_which_rules() {
    // A tenant that has never opened the billing settings: the paper still
    // prints, and the e-invoice names every rule that stops it existing.
    let h = harness("bill-fx-incomplete").await;
    common::seed_default_chart(&h.acc).await;
    let customer = a_customer(&h.app, &h.token, "Kunde GmbH").await;
    let invoice = a_draft(&h, &customer, "Consulting").await;
    issue(&h, &invoice).await;

    let refused = fetch(
        &h.app,
        Some(&h.token),
        &format!("/billing/invoices/{invoice}/facturx.xml"),
    )
    .await;
    assert_eq!(
        refused.status,
        StatusCode::UNPROCESSABLE_ENTITY,
        "{}",
        refused.text()
    );
    let detail = refused.text();
    for rule in ["BR-06", "BR-08", "BR-09", "BR-CO-26", "BR-S-02"] {
        assert!(detail.contains(rule), "{rule} is not reported: {detail}");
    }
    assert!(detail.contains("billing details"), "{detail}");

    // …and the PDF is an ordinary PDF rather than a failure.
    let pdf = fetch(
        &h.app,
        Some(&h.token),
        &format!("/billing/invoices/{invoice}/pdf"),
    )
    .await;
    assert_eq!(pdf.status, StatusCode::OK);
    assert!(!pdf.text().contains("factur-x.xml"));
}

// ---- the document itself -----------------------------------------------------

#[tokio::test]
async fn an_issued_invoice_downloads_as_an_en16931_document() {
    let (h, invoice, number) = a_complete_tenant("bill-fx-issued").await;
    let file = fetch(
        &h.app,
        Some(&h.token),
        &format!("/billing/invoices/{invoice}/facturx.xml"),
    )
    .await;
    assert_eq!(file.status, StatusCode::OK, "{}", file.text());

    // The response's own contract: a file, never sniffed, never cached.
    assert_eq!(
        file.header("content-type"),
        "application/xml; charset=utf-8"
    );
    assert_eq!(
        file.header("content-disposition"),
        format!("attachment; filename=\"Invoice-{number}-factur-x.xml\"")
    );
    assert_eq!(file.header("x-content-type-options"), "nosniff");
    assert_eq!(file.header("cache-control"), "no-store");

    let xml = file.text();
    assert!(xml.contains("<ram:ID>urn:cen.eu:en16931:2017</ram:ID>"));
    assert!(xml.contains(&format!("<ram:ID>{number}</ram:ID>")));
    assert!(xml.contains("<ram:TypeCode>380</ram:TypeCode>"));
    // The store's figures: 2 h at 120.00 is 240.00 net, 21 % is 50.40.
    assert!(xml.contains("<ram:LineTotalAmount>240.00</ram:LineTotalAmount>"));
    assert!(xml.contains("<ram:TaxTotalAmount currencyID=\"EUR\">50.40</ram:TaxTotalAmount>"));
    assert!(xml.contains("<ram:GrandTotalAmount>290.40</ram:GrandTotalAmount>"));
    assert!(xml.contains("<ram:DuePayableAmount>290.40</ram:DuePayableAmount>"));
    // Both parties, both VAT identifiers, both countries.
    assert!(xml.contains("<ram:ID schemeID=\"VA\">NL812345678B01</ram:ID>"));
    assert!(xml.contains("<ram:ID schemeID=\"VA\">DE811907980</ram:ID>"));
    assert!(xml.contains("<ram:IBANID>NL91ABNA0417164300</ram:IBANID>"));
}

#[tokio::test]
async fn the_pdf_carries_the_same_bytes_the_xml_route_serves() {
    let (h, invoice, _) = a_complete_tenant("bill-fx-hybrid").await;
    let xml = fetch(
        &h.app,
        Some(&h.token),
        &format!("/billing/invoices/{invoice}/facturx.xml"),
    )
    .await;
    let pdf = fetch(
        &h.app,
        Some(&h.token),
        &format!("/billing/invoices/{invoice}/pdf"),
    )
    .await;
    assert_eq!(pdf.status, StatusCode::OK);
    assert!(pdf.bytes.starts_with(b"%PDF-1.7"));

    // One invoice, one set of bytes, two ways of getting at it.
    assert_eq!(embedded_xml(&pdf.bytes), xml.text());

    // …and the PDF says what it carries, the way a reader looks for it.
    let raw = pdf.text();
    assert!(raw.contains("/AFRelationship /Alternative"));
    assert!(raw.contains("/Type /Filespec"));
    assert!(raw.contains("(factur-x.xml)"));
    assert!(raw.contains("/Type /Metadata /Subtype /XML"));
    assert!(raw.contains("<fx:ConformanceLevel>EN 16931</fx:ConformanceLevel>"));
    // The claim we do not make: this file is not PDF/A-3 yet (an embedded font
    // and an output intent are a human's licence decision).
    assert!(!raw.contains("pdfaid:part"));
}

#[tokio::test]
async fn a_credit_note_is_a_381_that_names_what_it_corrects() {
    let (h, invoice, number) = a_complete_tenant("bill-fx-credit").await;
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

    let xml = fetch(
        &h.app,
        Some(&h.token),
        &format!("/billing/invoices/{credit}/facturx.xml"),
    )
    .await
    .text();
    assert!(xml.contains("<ram:TypeCode>381</ram:TypeCode>"));
    assert!(xml.contains(&format!(
        "<ram:IssuerAssignedID>{number}</ram:IssuerAssignedID>"
    )));
    // Positive on the wire, negative in our ledger — the direction is the type
    // code's job. And nothing on it invites a payment.
    assert!(xml.contains("<ram:GrandTotalAmount>290.40</ram:GrandTotalAmount>"));
    assert!(
        !xml.contains(">-"),
        "a credit note carries no negative: {xml}"
    );
    assert!(!xml.contains("IBANID"));
    assert!(!xml.contains("PaymentReference"));
}

// ---- the mandatory wrong-tenant proof ----------------------------------------

#[tokio::test]
async fn no_byte_of_another_tenants_document_reaches_an_e_invoice() {
    let (a, a_invoice, _) = a_complete_tenant("bill-fx-a").await;
    let b = harness("bill-fx-b").await;
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
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let b_customer = a_customer(&b.app, &b.token, "B-SECRET-CUSTOMER").await;
    let b_invoice = a_draft(&b, &b_customer, "B-SECRET-WORK").await;
    issue(&b, &b_invoice).await;

    // B's document is a 404 to A — not a 403 and not a 409, either of which
    // would confirm the id exists — and the refusal leaks nothing.
    let refusal = fetch(
        &a.app,
        Some(&a.token),
        &format!("/billing/invoices/{b_invoice}/facturx.xml"),
    )
    .await;
    assert_eq!(refusal.status, StatusCode::NOT_FOUND);
    let refused = refusal.text();
    for secret in [
        "NACHBAR-SECRET",
        "NL999888778B01",
        "NL76INGB0006174254",
        "B-SECRET-CUSTOMER",
        "B-SECRET-WORK",
    ] {
        assert!(!refused.contains(secret), "{secret} leaked in: {refused}");
    }
    // An id that never existed anywhere gets the identical answer.
    let ghost = fetch(
        &a.app,
        Some(&a.token),
        "/billing/invoices/ghost/facturx.xml",
    )
    .await;
    assert_eq!(ghost.status, StatusCode::NOT_FOUND);
    assert_eq!(ghost.text(), refused);

    // And A's own e-invoice — served alone and embedded in its PDF — carries
    // nothing of B's, even though B's rows are in the same tables.
    let a_xml = fetch(
        &a.app,
        Some(&a.token),
        &format!("/billing/invoices/{a_invoice}/facturx.xml"),
    )
    .await
    .text();
    let a_pdf = fetch(
        &a.app,
        Some(&a.token),
        &format!("/billing/invoices/{a_invoice}/pdf"),
    )
    .await;
    for document in [&a_xml, &a_pdf.text()] {
        for secret in [
            "NACHBAR",
            "NL999888778B01",
            "INGB",
            "B-SECRET-CUSTOMER",
            "B-SECRET-WORK",
        ] {
            assert!(
                !document.contains(secret),
                "{secret} reached the neighbour's e-invoice"
            );
        }
    }
    assert!(a_xml.contains("Kunde &amp; S\u{f6}hne GmbH"));
}
