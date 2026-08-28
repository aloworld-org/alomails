//! The `GET /billing/invoices/{id}/pdf` route (B1.17), driven through the real
//! router over a real Postgres.
//!
//! The route serves a **file**, and the two things a file makes different from
//! every other billing response are what this suite is for:
//!
//! - **It is bytes, not JSON.** So the assertions are on the bytes: the magic
//!   number, the trailer, the size, and — through an independent PDF parser —
//!   the words a customer would actually read on the page.
//! - **It leaves our origin.** So the response's own contract matters: an
//!   attachment, never inline, with a file name built from the document rather
//!   than from anything a customer typed, and never cached.
//!
//! And the mandatory question, sharper here than on a JSON route because a
//! printed document is the one place in billing where **two records are
//! rendered together**: not only "can A fetch B's invoice" but "can a byte of
//! B's identity reach A's file". The neighbour below holds a distinctive legal
//! name and bank account, and every assertion looks for them.

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

    /// The document as an independent PDF parser reads it — what a customer
    /// sees, rather than what we wrote.
    fn text(&self) -> String {
        pdf_extract::extract_text_from_mem(&self.bytes)
            .unwrap_or_else(|e| panic!("the served PDF could not be read back: {e}"))
            .replace('\u{a0}', " ")
    }

    /// Whatever the response carries, as text — a PDF body or a `Problem`.
    fn as_text(&self) -> String {
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

// ---- fixtures ----------------------------------------------------------------

/// The tenant's own identity, with values distinctive enough that finding any
/// of them on another tenant's file is unambiguous.
fn identity(name: &str, iban: &str) -> Value {
    json!({
        "legalName": name,
        "addressLine1": "Keizersgracht 1",
        "postalCode": "1015 CJ",
        "city": "Amsterdam",
        "country": "NL",
        "vatId": "NL812345678B01",
        "registrationNo": "KVK 90123456",
        "email": "billing@alo.test",
        "iban": iban,
        "bic": "ABNANL2A",
        "bankName": "ABN AMRO",
        "footerNote": "Retention of title until paid in full.",
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
                "paymentTermsDays": 14,
            }),
        )
        .await,
    )
}

fn lines(description: &str) -> Value {
    json!([{
        "description": description,
        "unit": "hour",
        "qtyMilli": 2_000,
        "unitPriceCents": 12_000,
        "vatRateBp": 2_100,
    }])
}

async fn a_draft(h: &Harness, customer: &str, description: &str) -> String {
    created_id(
        "invoice",
        post(
            &h.app,
            &h.token,
            "/billing/invoices",
            json!({ "customerId": customer, "reference": "PO-42", "lines": lines(description) }),
        )
        .await,
    )
}

/// An issued invoice of `h`'s tenant, and the number it drew.
async fn an_issued_invoice(h: &Harness, customer: &str, description: &str) -> (String, String) {
    let id = a_draft(h, customer, description).await;
    let (status, body) = post(
        &h.app,
        &h.token,
        &format!("/billing/invoices/{id}/issue"),
        json!({}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "issue failed: {body}");
    let number = body["invoice"]["number"].as_str().unwrap().to_owned();
    (id, number)
}

// ---- guards ------------------------------------------------------------------

#[tokio::test]
async fn the_route_needs_a_token_and_an_id_that_exists() {
    let h = harness("bill-pdf-guards").await;
    common::seed_default_chart(&h.acc).await;

    // The auth guard runs before anything is looked up, so an unauthenticated
    // caller learns nothing about which ids exist.
    let anonymous = fetch(&h.app, None, "/billing/invoices/no-such-id/pdf").await;
    assert_eq!(anonymous.status, StatusCode::UNAUTHORIZED);
    assert!(!anonymous.bytes.starts_with(b"%PDF"));

    // With a token, a document that was never raised is a 404 — the same
    // answer another tenant's id gets below.
    let missing = fetch(&h.app, Some(&h.token), "/billing/invoices/no-such-id/pdf").await;
    assert_eq!(
        missing.status,
        StatusCode::NOT_FOUND,
        "{}",
        missing.as_text()
    );
}

#[tokio::test]
async fn an_issued_invoice_downloads_as_a_pdf_a_reader_can_open() {
    let h = harness("bill-pdf-issued").await;
    common::seed_default_chart(&h.acc).await;
    let (status, body) = patch(
        &h.app,
        &h.token,
        "/billing/settings",
        identity("Alo Werkplaats B.V.", "NL91ABNA0417164300"),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let customer = a_customer(&h.app, &h.token, "Kunde & Söhne GmbH").await;
    let (invoice, number) = an_issued_invoice(&h, &customer, "Consulting").await;

    let file = fetch(
        &h.app,
        Some(&h.token),
        &format!("/billing/invoices/{invoice}/pdf"),
    )
    .await;
    assert_eq!(file.status, StatusCode::OK, "{}", file.as_text());

    // A file, from its first byte to its last, and not a stub.
    assert!(file.bytes.starts_with(b"%PDF-1.7"), "not a PDF");
    assert!(file.bytes.ends_with(b"%%EOF\n"), "truncated");
    assert!(file.bytes.len() > 2_000, "{} bytes", file.bytes.len());

    // The response's own contract: a download, named after the document, that
    // no cache may keep and nothing may re-interpret.
    assert_eq!(file.header("content-type"), "application/pdf");
    assert_eq!(
        file.header("content-disposition"),
        format!("attachment; filename=\"Invoice-{number}.pdf\"")
    );
    assert_eq!(file.header("cache-control"), "no-store");
    assert_eq!(file.header("x-content-type-options"), "nosniff");

    // …and the page inside it is this invoice: both parties, the number, the
    // dates, the server's money, and the account it is paid into.
    let read = file.text();
    assert!(read.contains(&format!("Invoice {number}")), "{read}");
    assert!(read.contains("Kunde & Söhne GmbH"));
    assert!(read.contains("Alo Werkplaats B.V."));
    assert!(read.contains("Consulting"));
    assert!(
        read.contains("EUR 240.00") && read.contains("EUR 290.40"),
        "{read}"
    );
    assert!(read.contains("VAT 21%"));
    assert!(read.contains("NL91 ABNA 0417 1643 00"));
    assert!(read.contains("PO-42"));
    assert!(read.contains("DE811907980") && read.contains("NL812345678B01"));
}

#[tokio::test]
async fn a_draft_downloads_too_and_says_it_is_one() {
    // A draft has no number, so it must not print one — and the file it saves
    // as must not pretend to name a document that does not legally exist.
    let h = harness("bill-pdf-draft").await;
    common::seed_default_chart(&h.acc).await;
    patch(
        &h.app,
        &h.token,
        "/billing/settings",
        identity("Alo Werkplaats B.V.", "NL91ABNA0417164300"),
    )
    .await;
    let customer = a_customer(&h.app, &h.token, "Kunde GmbH").await;
    let draft = a_draft(&h, &customer, "Consulting").await;

    let file = fetch(
        &h.app,
        Some(&h.token),
        &format!("/billing/invoices/{draft}/pdf"),
    )
    .await;
    assert_eq!(file.status, StatusCode::OK);
    assert_eq!(
        file.header("content-disposition"),
        "attachment; filename=\"Invoice.pdf\""
    );
    let read = file.text();
    assert!(read.contains("DRAFT"), "{read}");
    assert!(!read.contains("INV-"), "a draft carries no number: {read}");
    assert!(read.contains("within 14 days"), "the term instead: {read}");
}

#[tokio::test]
async fn a_credit_note_is_titled_as_one_and_carries_no_bank_account() {
    let h = harness("bill-pdf-credit").await;
    common::seed_default_chart(&h.acc).await;
    patch(
        &h.app,
        &h.token,
        "/billing/settings",
        identity("Alo Werkplaats B.V.", "NL91ABNA0417164300"),
    )
    .await;
    let customer = a_customer(&h.app, &h.token, "Kunde GmbH").await;
    let (invoice, number) = an_issued_invoice(&h, &customer, "Consulting").await;
    let credit = created_id(
        "invoice",
        post(
            &h.app,
            &h.token,
            &format!("/billing/invoices/{invoice}/credit-note"),
            json!({}),
        )
        .await,
    );

    let file = fetch(
        &h.app,
        Some(&h.token),
        &format!("/billing/invoices/{credit}/pdf"),
    )
    .await;
    assert_eq!(file.status, StatusCode::OK);
    let read = file.text();
    assert!(read.contains("Credit note"), "{read}");
    assert!(
        read.contains(&format!("corrects invoice {number}")),
        "it must name the document the customer already holds: {read}"
    );
    assert!(read.contains("nothing is payable"));
    // An IBAN under "nothing is payable" is how a document gets paid twice.
    assert!(!read.contains("NL91 ABNA"), "{read}");
}

#[tokio::test]
async fn the_language_of_the_file_is_a_preference_and_never_a_refusal() {
    let h = harness("bill-pdf-lang").await;
    common::seed_default_chart(&h.acc).await;
    let customer = a_customer(&h.app, &h.token, "Kunde GmbH").await;
    let (invoice, _) = an_issued_invoice(&h, &customer, "Consulting").await;
    // A language we ship writes the file in it; anything else — an unknown
    // tag, a malformed one, an emoji — still produces a valid PDF, in English.
    // A display preference must never be the reason a document cannot be made.
    for (query, heading) in [
        ("", "Invoice INV-"),
        ("?lang=en-GB", "Invoice INV-"),
        ("?lang=fr", "Facture INV-"),
        ("?lang=fr-BE", "Facture INV-"),
        ("?lang=nl", "Factuur INV-"),
        ("?lang=xx-YY", "Invoice INV-"),
        ("?lang=%F0%9F%99%82", "Invoice INV-"),
    ] {
        let file = fetch(
            &h.app,
            Some(&h.token),
            &format!("/billing/invoices/{invoice}/pdf{query}"),
        )
        .await;
        assert_eq!(file.status, StatusCode::OK, "lang {query:?}");
        assert!(file.bytes.starts_with(b"%PDF-1.7"));
        assert!(file.text().contains(heading), "lang {query:?}");
    }
}

// ---- the mandatory wrong-tenant proof ----------------------------------------

#[tokio::test]
async fn neither_a_document_nor_an_identity_ever_crosses_a_tenant() {
    let a = harness("bill-pdf-a").await;
    common::seed_default_chart(&a.acc).await;
    let b = harness("bill-pdf-b").await;
    common::seed_default_chart(&b.acc).await;

    // B states an identity nobody else could plausibly have, and raises a
    // document with an equally distinctive line.
    let (status, body) = patch(
        &b.app,
        &b.token,
        "/billing/settings",
        identity("NACHBAR-SECRET GmbH", "NL76INGB0006174254"),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let b_customer = a_customer(&b.app, &b.token, "B-SECRET-CUSTOMER").await;
    let (b_issued, _) = an_issued_invoice(&b, &b_customer, "B-SECRET-WORK").await;
    let b_draft = a_draft(&b, &b_customer, "B-SECRET-WORK").await;

    // ---- B's documents are a 404 to A, issued or draft --------------------
    for id in [&b_issued, &b_draft] {
        let refusal = fetch(
            &a.app,
            Some(&a.token),
            &format!("/billing/invoices/{id}/pdf"),
        )
        .await;
        assert_eq!(
            refusal.status,
            StatusCode::NOT_FOUND,
            "another tenant's document must be a 404"
        );
        // Not a `403` and not a `409`: either would confirm the id exists.
        // And the refusal itself must leak nothing it declined.
        assert!(!refusal.bytes.starts_with(b"%PDF"), "a file was served");
        let refused = refusal.as_text();
        for secret in [
            "NACHBAR-SECRET",
            "NL76INGB0006174254",
            "B-SECRET-CUSTOMER",
            "B-SECRET-WORK",
        ] {
            assert!(!refused.contains(secret), "{secret} leaked in: {refused}");
        }
    }

    // An id that never existed anywhere gets the identical answer, so the
    // status is not an existence oracle either.
    let ghost = fetch(&a.app, Some(&a.token), "/billing/invoices/ghost/pdf").await;
    assert_eq!(ghost.status, StatusCode::NOT_FOUND);
    assert_eq!(ghost.as_text(), {
        let other = fetch(
            &a.app,
            Some(&a.token),
            &format!("/billing/invoices/{b_issued}/pdf"),
        )
        .await;
        other.as_text()
    });

    // ---- and A's own file carries nothing of B's --------------------------
    // A never saved an identity, so its document prints its own blanks even
    // though the settings table now has B's row in it.
    let a_customer_id = a_customer(&a.app, &a.token, "A-CUSTOMER").await;
    let (a_invoice, _) = an_issued_invoice(&a, &a_customer_id, "A-WORK").await;
    let file = fetch(
        &a.app,
        Some(&a.token),
        &format!("/billing/invoices/{a_invoice}/pdf"),
    )
    .await;
    assert_eq!(file.status, StatusCode::OK);
    let read = file.text();
    assert!(read.contains("A-CUSTOMER") && read.contains("A-WORK"));
    assert!(read.contains("have not been filled in yet"), "{read}");
    for secret in [
        "NACHBAR-SECRET",
        "NL76 INGB 0006 1742 54",
        "NL76INGB0006174254",
        "B-SECRET-CUSTOMER",
        "B-SECRET-WORK",
    ] {
        assert!(
            !read.contains(secret),
            "{secret} reached the neighbour's paper: {read}"
        );
    }
    // The whole file, not only its text: a byte of B's identity must not be
    // anywhere in it, metadata included.
    let raw = String::from_utf8_lossy(&file.bytes);
    assert!(!raw.contains("NACHBAR") && !raw.contains("INGB"));
}

// ---- the two renderings are one document -------------------------------------

#[tokio::test]
async fn the_file_and_the_page_say_the_same_thing() {
    // The reason the PDF is a second renderer over one model rather than a
    // conversion: whatever a customer is shown on screen is what they receive.
    let h = harness("bill-pdf-agree").await;
    common::seed_default_chart(&h.acc).await;
    patch(
        &h.app,
        &h.token,
        "/billing/settings",
        identity("Alo Werkplaats B.V.", "NL91ABNA0417164300"),
    )
    .await;
    let customer = a_customer(&h.app, &h.token, "Kunde & Söhne GmbH").await;
    let (invoice, number) = an_issued_invoice(&h, &customer, "Consulting").await;

    let page = fetch(
        &h.app,
        Some(&h.token),
        &format!("/billing/invoices/{invoice}/print"),
    )
    .await;
    assert_eq!(page.status, StatusCode::OK);
    let html = page.as_text();
    let pdf = fetch(
        &h.app,
        Some(&h.token),
        &format!("/billing/invoices/{invoice}/pdf"),
    )
    .await;
    let read = pdf.text();

    // Every figure and identifier that matters appears in both.
    for fact in [
        number.as_str(),
        "EUR 240.00",
        "EUR 290.40",
        "VAT 21%",
        "DE811907980",
        "NL812345678B01",
        "NL91 ABNA 0417 1643 00",
        "Payable by",
    ] {
        // The HTML groups digits with a narrow no-break space; the extracted
        // PDF text with an ordinary one. Compare on the same footing.
        let in_html = fact.replace(' ', "\u{202f}");
        assert!(
            html.contains(fact) || html.contains(&in_html),
            "{fact:?} is on the file but not the page"
        );
        assert!(
            read.contains(fact),
            "{fact:?} is on the page but not the file"
        );
    }
}
