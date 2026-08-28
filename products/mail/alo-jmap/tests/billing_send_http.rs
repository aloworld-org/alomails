//! The `POST /billing/invoices/{id}/send` route (B1.18), driven through the
//! real router over a real Postgres.
//!
//! This is the first billing route that **writes outside billing** — it puts a
//! message in the user's mailbox — so the suite asks three questions the other
//! billing suites do not:
//!
//! - **Is it really a draft?** Not "did the route answer 200", but: is there a
//!   message, in the Drafts folder, carrying `$draft`, with the customer as its
//!   recipient and the PDF actually attached as bytes a reader can open. The
//!   assertions go through the stored message, not the response body.
//! - **Did it stay off the wire?** Nothing here may send. The route is called
//!   and the account's Sent folder is examined afterwards.
//! - **Can a document reach a stranger?** The tenant question, sharpened: not
//!   only "is A's invoice a 404 for B", but "did a refused send leave anything
//!   at all in B's mailbox", and "does A's draft carry only A's data".

#![allow(clippy::unwrap_used, clippy::expect_used)]

use crate::common;

use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use base64::Engine;
use base64::engine::general_purpose::STANDARD as B64;
use serde_json::{Value, json};

use crate::common::{Harness, harness, send};
use alo_store::{AccountStore, MessageId};

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

fn created_id(kind: &str, (status, body): (StatusCode, Value)) -> String {
    assert_eq!(status, StatusCode::OK, "create failed: {body}");
    body[kind]["id"].as_str().unwrap().to_owned()
}

// ---- fixtures ----------------------------------------------------------------

/// The tenant's own identity. The legal name and the account are distinctive
/// enough that finding either in another tenant's mailbox is unambiguous.
fn identity(name: &str, iban: &str) -> Value {
    json!({
        "legalName": name,
        "addressLine1": "Keizersgracht 1",
        "postalCode": "1015 CJ",
        "city": "Amsterdam",
        "country": "NL",
        "vatId": "NL812345678B01",
        "email": "billing@alo.test",
        "iban": iban,
        "bic": "ABNANL2A",
        "bankName": "ABN AMRO",
    })
}

async fn a_customer(app: &Router, token: &str, name: &str, email: Option<&str>) -> String {
    let mut body = json!({
        "name": name,
        "addressLine1": "Hauptstraße 1",
        "postalCode": "10115",
        "city": "Berlin",
        "country": "DE",
        "vatId": "DE811907980",
        "paymentTermsDays": 14,
    });
    if let Some(email) = email {
        body["email"] = json!(email);
    }
    created_id(
        "customer",
        post(app, token, "/billing/customers", body).await,
    )
}

fn lines(description: &str) -> Value {
    json!([{
        "description": description,
        "unit": "hour",
        "qtyMilli": 12_500,
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

/// A tenant with its identity saved, one customer, and one issued invoice.
async fn ready(
    tag: &str,
    issuer: &str,
    iban: &str,
    customer_email: &str,
) -> (Harness, String, String) {
    let h = harness(tag).await;
    common::seed_default_chart(&h.acc).await;
    let (status, body) = patch(
        &h.app,
        &h.token,
        "/billing/settings",
        identity(issuer, iban),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let customer = a_customer(&h.app, &h.token, "Kunde & Söhne GmbH", Some(customer_email)).await;
    let (invoice, number) = an_issued_invoice(&h, &customer, "Consulting").await;
    (h, invoice, number)
}

// ---- reading the message that was written -----------------------------------

/// The stored draft, taken apart the way a mail client would: the raw message,
/// its decoded covering note, and its decoded attachment.
struct Draft {
    raw: String,
    text: String,
    attachment: Vec<u8>,
}

impl Draft {
    /// A top-level header's value, unfolded onto one line and RFC 2047-decoded
    /// — what a reader sees, not what the encoder wrote.
    fn header(&self, name: &str) -> String {
        let mut value = String::new();
        let mut reading = false;
        for line in self.raw.split("\r\n") {
            if line.is_empty() {
                break;
            }
            if reading {
                if line.starts_with(' ') || line.starts_with('\t') {
                    value.push(' ');
                    value.push_str(line.trim_start());
                    continue;
                }
                break;
            }
            let prefix = format!("{}:", name.to_ascii_lowercase());
            if line.to_ascii_lowercase().starts_with(&prefix) {
                value = line[name.len() + 1..].trim().to_owned();
                reading = true;
            }
        }
        // A header that folded onto its own continuation line starts with the
        // fold's space; unfolding is not the place to keep it.
        alo_store::rfc2047::decode(value.trim())
    }

    /// The attached document as an independent PDF parser reads it.
    fn document(&self) -> String {
        pdf_extract::extract_text_from_mem(&self.attachment)
            .unwrap_or_else(|e| panic!("the attached PDF could not be read back: {e}"))
            .replace('\u{a0}', " ")
    }
}

/// One MIME part's decoded body, honouring its own transfer encoding.
fn part_body(part: &str) -> Vec<u8> {
    let (headers, body) = part
        .split_once("\r\n\r\n")
        .expect("a part has a blank line");
    if headers
        .to_ascii_lowercase()
        .contains("content-transfer-encoding: base64")
    {
        let encoded: String = body
            .split("\r\n")
            .take_while(|line| !line.starts_with("--"))
            .collect();
        return B64.decode(encoded.trim()).expect("base64 part");
    }
    body.split("\r\n")
        .take_while(|line| !line.starts_with("--"))
        .collect::<Vec<_>>()
        .join("\r\n")
        .into_bytes()
}

/// Reads a saved draft out of the store — the message itself, not the route's
/// account of it — and takes its multipart body apart.
async fn stored_draft(acc: &AccountStore, id: &str) -> Draft {
    let bytes = acc
        .message_bytes(&MessageId::new(id.to_owned()))
        .await
        .expect("the draft the route reported must exist");
    let raw = String::from_utf8_lossy(&bytes).into_owned();
    // The parts of the multipart/mixed body, in order: the covering note, then
    // the attachment. Each chunk still carries the rest of the boundary line,
    // which is dropped with it.
    let parts: Vec<String> = raw
        .split("\r\n--=_mix_")
        .skip(1)
        .filter_map(|chunk| chunk.split_once("\r\n").map(|(_, part)| part.to_owned()))
        .filter(|part| part.contains("\r\n\r\n"))
        .collect();
    assert_eq!(parts.len(), 2, "a covering note and one attachment: {raw}");
    let text = String::from_utf8(part_body(&parts[0])).expect("a utf-8 note");
    let attachment = part_body(&parts[1]);
    Draft {
        raw,
        text,
        attachment,
    }
}

/// How many messages sit in a role mailbox of an account (`None` when the
/// account has no such folder at all).
async fn role_count(acc: &AccountStore, role: &str) -> Option<i64> {
    let id = acc.mailbox_by_role(role).await.expect("mailbox lookup")?;
    Some(acc.mailbox(&id).await.expect("mailbox").total_messages)
}

// ---- guards ------------------------------------------------------------------

#[tokio::test]
async fn the_route_needs_a_token_and_an_id_that_exists() {
    let h = harness("bill-send-guards").await;
    common::seed_default_chart(&h.acc).await;

    let (status, _) = send(
        &h.app,
        with_json("POST", "/billing/invoices/no-such-id/send", None, json!({})),
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);

    let (status, body) = post(
        &h.app,
        &h.token,
        "/billing/invoices/no-such-id/send",
        json!({}),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND, "{body}");
    // Nothing was written for a request that never named a real document.
    assert_eq!(role_count(&h.acc, "drafts").await, None);
}

#[tokio::test]
async fn a_document_the_customer_should_not_see_is_refused_by_state() {
    let h = harness("bill-send-state").await;
    common::seed_default_chart(&h.acc).await;
    let customer = a_customer(&h.app, &h.token, "Kunde GmbH", Some("k@kunde.test")).await;

    // A draft carries no number and prints a DRAFT banner.
    let draft = a_draft(&h, &customer, "Consulting").await;
    let (status, body) = post(
        &h.app,
        &h.token,
        &format!("/billing/invoices/{draft}/send"),
        json!({}),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    assert!(
        body["detail"]
            .as_str()
            .unwrap_or_default()
            .contains("issue it first"),
        "{body}"
    );

    // …and a cancelled one is corrected with a credit note, not mailed.
    let (invoice, _) = an_issued_invoice(&h, &customer, "Consulting").await;
    let (status, body) = post(
        &h.app,
        &h.token,
        &format!("/billing/invoices/{invoice}/void"),
        json!({}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let (status, body) = post(
        &h.app,
        &h.token,
        &format!("/billing/invoices/{invoice}/send"),
        json!({}),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT, "{body}");

    // Two refusals, and not so much as a Drafts folder created between them.
    assert_eq!(role_count(&h.acc, "drafts").await, None);
}

#[tokio::test]
async fn a_customer_with_no_address_is_a_422_naming_the_reason() {
    let h = harness("bill-send-no-addr").await;
    common::seed_default_chart(&h.acc).await;
    let customer = a_customer(&h.app, &h.token, "Kunde GmbH", None).await;
    let (invoice, _) = an_issued_invoice(&h, &customer, "Consulting").await;

    let (status, body) = post(
        &h.app,
        &h.token,
        &format!("/billing/invoices/{invoice}/send"),
        json!({}),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
    assert_eq!(
        body["detail"].as_str(),
        Some("this customer has no email address")
    );
    assert_eq!(role_count(&h.acc, "drafts").await, None);
}

// ---- the draft it writes -----------------------------------------------------

#[tokio::test]
async fn an_issued_invoice_becomes_a_draft_with_the_document_attached() {
    let (h, invoice, number) = ready(
        "bill-send-draft",
        "Alo Werkplaats B.V.",
        "NL91ABNA0417164300",
        "buchhaltung@kunde.test",
    )
    .await;

    let (status, body) = post(
        &h.app,
        &h.token,
        &format!("/billing/invoices/{invoice}/send"),
        json!({}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");

    // What the route says it did.
    let draft_id = body["draft"]["id"].as_str().unwrap().to_owned();
    assert_eq!(body["draft"]["to"], json!("buchhaltung@kunde.test"));
    assert_eq!(
        body["draft"]["subject"],
        json!(format!("Invoice {number} \u{2014} Alo Werkplaats B.V."))
    );
    assert_eq!(
        body["draft"]["attachment"]["name"],
        json!(format!("Invoice-{number}.pdf"))
    );
    assert!(
        body["draft"]["attachment"]["sizeBytes"]
            .as_i64()
            .unwrap_or(0)
            > 2_000,
        "{body}"
    );

    // What is actually in the mailbox.
    let drafts = h
        .acc
        .mailbox_by_role("drafts")
        .await
        .unwrap()
        .expect("Drafts");
    assert_eq!(h.acc.mailbox(&drafts).await.unwrap().total_messages, 1);
    let message = MessageId::new(draft_id.clone());
    assert!(
        h.acc
            .mailboxes_of_message(&message)
            .await
            .unwrap()
            .iter()
            .any(|m| m.as_str() == drafts.as_str()),
        "the message must live in Drafts"
    );
    assert!(
        h.acc
            .keywords(&message)
            .await
            .unwrap()
            .contains(&"$draft".to_owned()),
        "a message without $draft is not submittable and is not a draft"
    );
    // Nothing was sent: there is no Sent folder, because nothing put one there.
    assert_eq!(role_count(&h.acc, "sent").await, None);

    let stored = stored_draft(&h.acc, &draft_id).await;
    // Addressed to the customer, from the caller's own address — never the
    // other way round, and never an address a request chose.
    assert!(
        stored.header("To").contains("buchhaltung@kunde.test"),
        "{}",
        stored.header("To")
    );
    assert!(
        stored.header("From").contains(&h.email),
        "{}",
        stored.header("From")
    );
    assert_eq!(
        stored.header("Subject"),
        format!("Invoice {number} \u{2014} Alo Werkplaats B.V.")
    );
    assert!(stored.header("Content-Type").starts_with("multipart/mixed"));

    // The covering note, in the words of the document it carries.
    assert!(
        stored
            .text
            .contains(&format!("Please find attached Invoice {number}")),
        "{}",
        stored.text
    );
    assert!(
        stored.text.contains("Your reference: PO-42"),
        "{}",
        stored.text
    );
    assert!(stored.text.contains("Kind regards,"), "{}", stored.text);

    // And the attachment is the document itself — read back with a parser that
    // knows nothing about how we wrote it.
    assert!(stored.attachment.starts_with(b"%PDF-1.7"), "not a PDF");
    assert!(stored.attachment.ends_with(b"%%EOF\n"), "truncated");
    let read = stored.document();
    assert!(read.contains(&format!("Invoice {number}")), "{read}");
    assert!(read.contains("Kunde & Söhne GmbH"), "{read}");
    assert!(read.contains("Alo Werkplaats B.V."));
    // 12.5 × 120.00 = 1 500.00 net, 21% VAT → 1 815.00 gross, and the same
    // figure is in the note beside it.
    assert!(read.contains("EUR 1 815.00"), "{read}");
    assert!(
        stored.text.contains("EUR 1\u{202f}815.00"),
        "{}",
        stored.text
    );
}

#[tokio::test]
async fn sending_twice_writes_two_drafts_and_changes_no_billing_record() {
    let (h, invoice, number) = ready(
        "bill-send-twice",
        "Alo Werkplaats B.V.",
        "NL91ABNA0417164300",
        "buchhaltung@kunde.test",
    )
    .await;
    let uri = format!("/billing/invoices/{invoice}/send");
    for _ in 0..2 {
        let (status, body) = post(&h.app, &h.token, &uri, json!({})).await;
        assert_eq!(status, StatusCode::OK, "{body}");
    }
    assert_eq!(role_count(&h.acc, "drafts").await, Some(2));

    // The invoice is untouched: same number, same status, still not paid by
    // anybody's account of having "sent" it.
    let (status, body) = send(
        &h.app,
        with_json(
            "GET",
            &format!("/billing/invoices/{invoice}"),
            Some(&h.token),
            json!({}),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["invoice"]["number"], json!(number));
    assert_eq!(body["invoice"]["status"], json!("issued"));
}

#[tokio::test]
async fn a_credit_note_is_covered_by_a_note_that_names_what_it_corrects() {
    let (h, invoice, number) = ready(
        "bill-send-credit",
        "Alo Werkplaats B.V.",
        "NL91ABNA0417164300",
        "buchhaltung@kunde.test",
    )
    .await;
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
    let (status, body) = post(
        &h.app,
        &h.token,
        &format!("/billing/invoices/{credit}/issue"),
        json!({}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let credit_number = body["invoice"]["number"].as_str().unwrap().to_owned();

    let (status, body) = post(
        &h.app,
        &h.token,
        &format!("/billing/invoices/{credit}/send"),
        json!({}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(
        body["draft"]["attachment"]["name"],
        json!(format!("Credit-note-{credit_number}.pdf"))
    );

    let stored = stored_draft(&h.acc, body["draft"]["id"].as_str().unwrap()).await;
    assert!(
        stored
            .text
            .contains(&format!("Please find attached Credit note {credit_number}")),
        "{}",
        stored.text
    );
    assert!(
        stored
            .text
            .contains(&format!("which corrects invoice {number}")),
        "{}",
        stored.text
    );
    // A credit note owes nothing, so neither the note nor the document offers
    // an account to pay it into.
    assert!(!stored.text.contains("payable by"), "{}", stored.text);
    assert!(
        !stored.document().contains("NL91 ABNA"),
        "{}",
        stored.document()
    );
}

// ---- the tenant question -----------------------------------------------------

#[tokio::test]
async fn a_stranger_can_neither_send_a_document_nor_appear_in_one() {
    let (a, invoice, number) = ready(
        "bill-send-tenant-a",
        "Alo Werkplaats B.V.",
        "NL91ABNA0417164300",
        "buchhaltung@kunde.test",
    )
    .await;
    let (b, _, _) = ready(
        "bill-send-tenant-b",
        "Nachbar Neighbour Holding BV",
        "BE68539007547034",
        "post@nachbar.test",
    )
    .await;

    // B asks for A's invoice by its real id: the same 404 a ghost id gets, so
    // the status is not an existence oracle.
    let (status, body) = post(
        &b.app,
        &b.token,
        &format!("/billing/invoices/{invoice}/send"),
        json!({}),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND, "{body}");
    let (ghost_status, ghost_body) =
        post(&b.app, &b.token, "/billing/invoices/ghost/send", json!({})).await;
    assert_eq!(ghost_status, status);
    assert_eq!(ghost_body["detail"], body["detail"]);
    // The refusal left nothing in B's mailbox.
    assert_eq!(role_count(&b.acc, "drafts").await, None);

    // And A's own draft, written after B filled its identity in, carries no
    // trace of B — not in the note, not in the attached document.
    let (status, body) = post(
        &a.app,
        &a.token,
        &format!("/billing/invoices/{invoice}/send"),
        json!({}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let stored = stored_draft(&a.acc, body["draft"]["id"].as_str().unwrap()).await;
    let document = stored.document();
    for secret in [
        "Nachbar",
        "Neighbour",
        "BE68",
        "post@nachbar.test",
        b.email.as_str(),
    ] {
        assert!(!stored.raw.contains(secret), "{secret} reached A's message");
        assert!(!stored.text.contains(secret), "{secret} reached A's note");
        assert!(!document.contains(secret), "{secret} reached A's document");
    }
    assert!(
        document.contains(&format!("Invoice {number}")),
        "{document}"
    );
    assert!(document.contains("Alo Werkplaats B.V."));
}
