//! The `/billing/settings` and `/billing/{invoices,quotes}/{id}/print` HTTP
//! surface (B1.16), driven through the real router over a real Postgres.
//!
//! `alo-store`'s own suite proves the issuer identity is tenant-scoped at the
//! door; what this suite is for is the **edge**, and one thing that only exists
//! here: the printed page is the one place in billing where **two records are
//! rendered into one document**. So the isolation question is not only "can A
//! fetch B's invoice" but "can a byte of B's identity reach A's paper" — the
//! neighbour in these tests holds a distinctive bank account and legal name,
//! and every assertion looks for them.
//!
//! Also the response's own contract: the three headers the design note treats
//! as a security control, and the `text/html` body that is not JSON.

#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use serde_json::{Value, json};
use tower::ServiceExt;

use common::{Harness, harness, send};

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

async fn get(app: &Router, token: &str, uri: &str) -> (StatusCode, Value) {
    send(app, with_json("GET", uri, Some(token), json!({}))).await
}

/// A printed page: the status, the headers that matter, and the HTML itself.
///
/// `common::send` parses every body as JSON, which a document is not — this is
/// the raw read, and it is also the only way to see the response headers.
struct Page {
    status: StatusCode,
    headers: Vec<(String, String)>,
    html: String,
}

impl Page {
    fn header(&self, name: &str) -> String {
        self.headers
            .iter()
            .find(|(k, _)| k == name)
            .map_or_else(String::new, |(_, v)| v.clone())
    }
}

async fn fetch_page(app: &Router, token: Option<&str>, uri: &str) -> Page {
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
        .unwrap();
    Page {
        status,
        headers,
        html: String::from_utf8_lossy(&bytes).into_owned(),
    }
}

fn created_id(kind: &str, (status, body): (StatusCode, Value)) -> String {
    assert_eq!(status, StatusCode::OK, "create failed: {body}");
    body[kind]["id"].as_str().unwrap().to_owned()
}

// ---- fixtures ----------------------------------------------------------------

/// The tenant's own identity, with values distinctive enough that finding any
/// of them on another tenant's page is unambiguous.
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

fn one_line(description: &str) -> Value {
    json!([{
        "description": description,
        "unit": "hour",
        "qtyMilli": 2_000,
        "unitPriceCents": 12_000,
        "vatRateBp": 2_100,
    }])
}

/// An issued invoice for a customer of `h`'s tenant, and its number.
async fn an_issued_invoice(h: &Harness, customer: &str, description: &str) -> (String, String) {
    let id = created_id(
        "invoice",
        post(
            &h.app,
            &h.token,
            "/billing/invoices",
            json!({ "customerId": customer, "reference": "PO-42", "lines": one_line(description) }),
        )
        .await,
    );
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
async fn every_new_route_needs_a_token_and_an_id_that_exists() {
    let h = harness("bill-print-guards").await;
    common::seed_default_chart(&h.acc).await;
    let routes: Vec<(&str, &str)> = vec![
        ("GET", "/billing/settings"),
        ("PATCH", "/billing/settings"),
        ("GET", "/billing/invoices/no-such-id/print"),
        ("GET", "/billing/quotes/no-such-id/print"),
    ];

    // No token: every route, including the ones that would otherwise 404 — the
    // auth guard runs before anything is looked up, so an unauthenticated
    // caller learns nothing about which ids exist.
    for (method, uri) in &routes {
        let (status, answer) = send(&h.app, with_json(method, uri, None, json!({}))).await;
        assert_eq!(
            status,
            StatusCode::UNAUTHORIZED,
            "{method} {uri} → {answer}"
        );
    }

    // With a token, a document that was never raised is a 404 — the same
    // answer another tenant's id gets below.
    for (_, uri) in routes.iter().filter(|(_, uri)| uri.contains("no-such-id")) {
        let page = fetch_page(&h.app, Some(&h.token), uri).await;
        assert_eq!(page.status, StatusCode::NOT_FOUND, "{uri} → {}", page.html);
    }
}

#[tokio::test]
async fn a_printed_page_carries_the_headers_that_keep_it_self_contained() {
    let h = harness("bill-print-headers").await;
    common::seed_default_chart(&h.acc).await;
    patch(
        &h.app,
        &h.token,
        "/billing/settings",
        identity("Alo B.V.", "NL91ABNA0417164300"),
    )
    .await;
    let customer = a_customer(&h.app, &h.token, "Acme GmbH").await;
    let (id, number) = an_issued_invoice(&h, &customer, "Consulting").await;

    let page = fetch_page(
        &h.app,
        Some(&h.token),
        &format!("/billing/invoices/{id}/print"),
    )
    .await;
    assert_eq!(page.status, StatusCode::OK);
    assert!(page.header("content-type").starts_with("text/html"));

    // The design note treats these three as a security control, so they are
    // asserted rather than assumed: nothing may leave the page, nothing may
    // re-interpret it, and a customer's invoice does not sit in a cache.
    let csp = page.header("content-security-policy");
    assert!(csp.contains("default-src 'none'"), "CSP was: {csp}");
    assert!(csp.contains("form-action 'none'"), "CSP was: {csp}");
    assert_eq!(page.header("x-content-type-options"), "nosniff");
    assert_eq!(page.header("cache-control"), "no-store");

    // And it really is the document: the number in the title, the issuer's
    // bank account grouped the way one is read out loud, and the server's own
    // gross (2 h at 120.00 plus 21 %).
    assert!(
        page.html
            .contains(&format!("<title>Invoice {number}</title>"))
    );
    assert!(page.html.contains("NL91 ABNA 0417 1643 00"));
    assert!(page.html.contains("EUR 290.40"), "{}", page.html);
}

// ---- tenancy -----------------------------------------------------------------

#[tokio::test]
async fn neither_the_identity_nor_the_paper_ever_crosses_a_tenant() {
    let a = harness("bill-print-a").await;
    common::seed_default_chart(&a.acc).await;
    let b = harness("bill-print-b").await;
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
    let (b_invoice, _) = an_issued_invoice(&b, &b_customer, "B-SECRET-WORK").await;
    let b_quote = created_id(
        "quote",
        post(
            &b.app,
            &b.token,
            "/billing/quotes",
            json!({ "customerId": b_customer, "lines": one_line("B-SECRET-WORK") }),
        )
        .await,
    );

    // ---- A reads its own blanks, not B's identity ------------------------
    let (status, body) = get(&a.app, &a.token, "/billing/settings").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["settings"]["stated"], false);
    assert_eq!(body["settings"]["legalName"], "");
    assert_eq!(body["settings"]["iban"], Value::Null);
    assert!(
        !body.to_string().contains("NACHBAR-SECRET")
            && !body.to_string().contains("NL76INGB0006174254"),
        "the neighbour's identity leaked: {body}"
    );

    // ---- B's documents are a 404 to A, on both print routes --------------
    for uri in [
        format!("/billing/invoices/{b_invoice}/print"),
        format!("/billing/quotes/{b_quote}/print"),
    ] {
        let page = fetch_page(&a.app, Some(&a.token), &uri).await;
        assert_eq!(page.status, StatusCode::NOT_FOUND, "{uri} → {}", page.html);
        assert!(
            !page.html.contains("B-SECRET") && !page.html.contains("NACHBAR-SECRET"),
            "{uri} leaked what it refused: {}",
            page.html
        );
    }

    // ---- and A's own paper is made of A's identity alone -----------------
    let (status, body) = patch(
        &a.app,
        &a.token,
        "/billing/settings",
        identity("Alo Werkplaats B.V.", "NL91ABNA0417164300"),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let a_customer = a_customer(&a.app, &a.token, "Acme GmbH").await;
    let (a_invoice, _) = an_issued_invoice(&a, &a_customer, "Consulting").await;
    let page = fetch_page(
        &a.app,
        Some(&a.token),
        &format!("/billing/invoices/{a_invoice}/print"),
    )
    .await;
    assert_eq!(page.status, StatusCode::OK);
    assert!(page.html.contains("Alo Werkplaats B.V."));
    assert!(page.html.contains("NL91 ABNA 0417 1643 00"));
    // The whole point of this suite: two records are rendered into one
    // document, and neither of them may be the neighbour's.
    for secret in [
        "NACHBAR-SECRET",
        "NL76INGB0006174254",
        "NL76 INGB 0006 1742 54",
        "B-SECRET",
    ] {
        assert!(
            !page.html.contains(secret),
            "A's invoice printed B's {secret}"
        );
    }

    // ---- B's identity survives everything A did --------------------------
    let (_, body) = get(&b.app, &b.token, "/billing/settings").await;
    assert_eq!(body["settings"]["legalName"], "NACHBAR-SECRET GmbH");
    assert_eq!(body["settings"]["iban"], "NL76INGB0006174254");
}

// ---- the identity as a resource ----------------------------------------------

#[tokio::test]
async fn the_identity_is_created_by_its_first_save_and_merged_afterwards() {
    let h = harness("bill-print-settings").await;
    common::seed_default_chart(&h.acc).await;

    // Never saved: the blanks, and it says so — never a 404 for a record with
    // exactly one row per tenant.
    let (status, body) = get(&h.app, &h.token, "/billing/settings").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["settings"]["stated"], false);
    assert_eq!(body["settings"]["updatedAt"], Value::Null);

    // A document that does not name its issuer is not an invoice.
    let (status, answer) = patch(&h.app, &h.token, "/billing/settings", json!({})).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{answer}");
    assert!(answer["detail"].as_str().unwrap().contains("legal name"));

    // The bank details are held to their own standard, and the refusal names
    // the rule rather than echoing what was typed.
    for (body, expect) in [
        (
            json!({ "legalName": "Alo", "iban": "NL92ABNA0417164300" }),
            "check digits",
        ),
        (json!({ "legalName": "Alo", "bic": "ABNANL2" }), "BIC"),
        (
            json!({ "legalName": "Alo", "vatId": "NL812345678B01" }),
            "country before the VAT id",
        ),
    ] {
        let (status, answer) = patch(&h.app, &h.token, "/billing/settings", body).await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{answer}");
        assert!(
            answer["detail"].as_str().unwrap().contains(expect),
            "expected {expect:?}, got {answer}"
        );
    }
    // Nothing was half-written by any of those.
    let (_, body) = get(&h.app, &h.token, "/billing/settings").await;
    assert_eq!(body["settings"]["stated"], false);

    // The real save canonicalises everything on the way in.
    let (status, body) = patch(
        &h.app,
        &h.token,
        "/billing/settings",
        identity("Alo Werkplaats B.V.", "nl91 abna 0417 1643 00"),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["settings"]["stated"], true);
    assert_eq!(body["settings"]["iban"], "NL91ABNA0417164300");
    assert_eq!(body["settings"]["vatId"], "NL812345678B01");
    assert!(body["settings"]["updatedAt"].is_string());

    // A PATCH is a merge, not a replace: what it does not mention survives.
    let (status, body) = patch(
        &h.app,
        &h.token,
        "/billing/settings",
        json!({ "city": "Rotterdam" }),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["settings"]["city"], "Rotterdam");
    assert_eq!(body["settings"]["iban"], "NL91ABNA0417164300");
    assert_eq!(body["settings"]["legalName"], "Alo Werkplaats B.V.");

    // …and `null` is how a nullable field comes off the record.
    let (_, body) = patch(
        &h.app,
        &h.token,
        "/billing/settings",
        json!({ "iban": null, "bic": null }),
    )
    .await;
    assert_eq!(body["settings"]["iban"], Value::Null);
    assert_eq!(body["settings"]["bic"], Value::Null);

    // With no account stated, an invoice still prints — and prints no bank
    // block rather than an empty one that reads like a missing field.
    let customer = a_customer(&h.app, &h.token, "Acme GmbH").await;
    let (id, _) = an_issued_invoice(&h, &customer, "Consulting").await;
    let page = fetch_page(
        &h.app,
        Some(&h.token),
        &format!("/billing/invoices/{id}/print"),
    )
    .await;
    assert_eq!(page.status, StatusCode::OK);
    assert!(page.html.contains("Alo Werkplaats B.V."));
    assert!(!page.html.contains("IBAN"), "{}", page.html);
}

// ---- what the paper says about itself ----------------------------------------

#[tokio::test]
async fn a_document_prints_as_what_it_actually_is() {
    let h = harness("bill-print-states").await;
    common::seed_default_chart(&h.acc).await;
    patch(
        &h.app,
        &h.token,
        "/billing/settings",
        identity("Alo Werkplaats B.V.", "NL91ABNA0417164300"),
    )
    .await;
    let customer = a_customer(&h.app, &h.token, "Acme GmbH").await;

    // A draft: no number, and it says so across the page.
    let draft = created_id(
        "invoice",
        post(
            &h.app,
            &h.token,
            "/billing/invoices",
            json!({ "customerId": customer, "lines": one_line("Consulting") }),
        )
        .await,
    );
    let page = fetch_page(
        &h.app,
        Some(&h.token),
        &format!("/billing/invoices/{draft}/print"),
    )
    .await;
    assert!(page.html.contains("class=\"banner\">Draft<"));
    assert!(!page.html.contains("INV-"), "a draft printed a number");
    // No due date yet, so it states the term instead of simply omitting when
    // the money is owed.
    assert!(page.html.contains("within 14 days"));

    // Issued, then credited: the credit note is titled as one, names what it
    // corrects, and shows no bank account — nothing is payable on it.
    let (issued, number) = an_issued_invoice(&h, &customer, "Consulting").await;
    let credit = created_id(
        "invoice",
        post(
            &h.app,
            &h.token,
            &format!("/billing/invoices/{issued}/credit-note"),
            json!({}),
        )
        .await,
    );
    let page = fetch_page(
        &h.app,
        Some(&h.token),
        &format!("/billing/invoices/{credit}/print"),
    )
    .await;
    assert!(page.html.contains("<title>Credit note</title>"));
    assert!(page.html.contains(&format!("corrects invoice {number}")));
    assert!(page.html.contains("nothing is payable"));
    assert!(
        !page.html.contains("NL91 ABNA"),
        "a credit note showed the bank account"
    );

    // A void invoice keeps its number and says it is void.
    let (voidable, void_number) = an_issued_invoice(&h, &customer, "Consulting").await;
    post(
        &h.app,
        &h.token,
        &format!("/billing/invoices/{voidable}/void"),
        json!({}),
    )
    .await;
    let page = fetch_page(
        &h.app,
        Some(&h.token),
        &format!("/billing/invoices/{voidable}/print"),
    )
    .await;
    assert!(page.html.contains("class=\"banner\">Void<"));
    assert!(page.html.contains(&void_number));

    // A sent quote is dated as an offer, owes nothing, and shows no account.
    let quote = created_id(
        "quote",
        post(
            &h.app,
            &h.token,
            "/billing/quotes",
            json!({ "customerId": customer, "lines": one_line("Consulting") }),
        )
        .await,
    );
    let (status, body) = post(
        &h.app,
        &h.token,
        &format!("/billing/quotes/{quote}/send"),
        json!({}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let page = fetch_page(
        &h.app,
        Some(&h.token),
        &format!("/billing/quotes/{quote}/print"),
    )
    .await;
    assert!(page.html.contains("<title>Quote QUO-"));
    assert!(page.html.contains("stands until"));
    assert!(!page.html.contains("Due date"));
    assert!(
        !page.html.contains("NL91 ABNA"),
        "a quote showed the bank account"
    );

    // A shipped language prints the document in it (B1.27), matched on the
    // primary subtag; an unknown one prints in the default rather than
    // refusing, because a display preference must never be why a document
    // cannot be printed. The page's `lang` attribute and its heading always
    // agree — a French document announcing itself as English is what breaks a
    // screen reader and a PDF's text extraction alike.
    for (lang, tag, heading) in [
        ("", "en", "Invoice INV-"),
        ("en", "en", "Invoice INV-"),
        ("en-GB", "en", "Invoice INV-"),
        ("fr", "fr", "Facture INV-"),
        ("fr-BE", "fr", "Facture INV-"),
        ("nl", "nl", "Factuur INV-"),
        ("NL", "nl", "Factuur INV-"),
        ("xx-YY", "en", "Invoice INV-"),
    ] {
        let page = fetch_page(
            &h.app,
            Some(&h.token),
            &format!("/billing/invoices/{issued}/print?lang={lang}"),
        )
        .await;
        assert_eq!(page.status, StatusCode::OK, "?lang={lang}");
        assert!(
            page.html.contains(&format!("<html lang=\"{tag}\">")),
            "?lang={lang}"
        );
        assert!(
            page.html.contains(&format!("<title>{heading}")),
            "?lang={lang}"
        );
    }
}

// ---- what a hostile record cannot do to the page -----------------------------

/// A value carrying markup, both quote characters and an apostrophe, tagged so
/// each field can be found individually on the page.
///
/// Short on purpose: a line's unit is bounded at 32 characters, and a fixture
/// that cannot be stored proves nothing about escaping.
fn hostile(tag: &str) -> String {
    format!("{tag} <b>'&\"x\"</b>")
}

/// The same value as it must appear on the page, and nowhere else.
fn escaped(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

#[tokio::test]
async fn nothing_a_user_can_type_becomes_markup() {
    let h = harness("bill-print-escaping").await;
    common::seed_default_chart(&h.acc).await;

    // Every free-text field of both records and of the document — twenty of
    // them. The page has one escaper; this is what proves it is on all twenty
    // rather than on the three a spot check would reach. The fields left out
    // are the ones no user can make hostile: country (two letters), currency
    // (three), the VAT id, the IBAN, the BIC and the email are each held to a
    // shape by the store before they can be stored at all.
    let (status, body) = patch(
        &h.app,
        &h.token,
        "/billing/settings",
        json!({
            "legalName": hostile("Issuer"),
            "addressLine1": hostile("Street"),
            "addressLine2": hostile("Suite"),
            "postalCode": hostile("PC"),
            "city": hostile("City"),
            "country": "NL",
            "registrationNo": hostile("Reg"),
            "email": "billing@alo.test",
            "phone": hostile("Phone"),
            "website": hostile("Web"),
            // The bank block only prints when there is an account to print.
            "iban": "NL91ABNA0417164300",
            "bic": "ABNANL2A",
            "bankName": hostile("Bank"),
            "accountHolder": hostile("Holder"),
            "footerNote": hostile("Footer"),
        }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");

    let customer = created_id(
        "customer",
        post(
            &h.app,
            &h.token,
            "/billing/customers",
            json!({
                "name": hostile("Customer"),
                "addressLine1": hostile("Their street"),
                "addressLine2": hostile("Their suite"),
                "postalCode": hostile("PC"),
                "city": hostile("Their city"),
                "country": "DE",
                "paymentTermsDays": 14,
            }),
        )
        .await,
    );
    let id = created_id(
        "invoice",
        post(
            &h.app,
            &h.token,
            "/billing/invoices",
            json!({
                "customerId": customer,
                "reference": hostile("Ref"),
                "note": hostile("Note"),
                "lines": [{
                    "description": hostile("Line"),
                    "unit": hostile("Unit"),
                    "qtyMilli": 2_000,
                    "unitPriceCents": 12_000,
                    "vatRateBp": 2_100,
                }],
            }),
        )
        .await,
    );

    let page = fetch_page(
        &h.app,
        Some(&h.token),
        &format!("/billing/invoices/{id}/print"),
    )
    .await;
    assert_eq!(page.status, StatusCode::OK);

    // Every one of the twenty is on the page, and every one of them is
    // escaped — a field that is silently not printed fails here too.
    for tag in [
        "Issuer",
        "Street",
        "Suite",
        "PC",
        "City",
        "Reg",
        "Phone",
        "Web",
        "Bank",
        "Holder",
        "Footer",
        "Customer",
        "Their street",
        "Their suite",
        "Their city",
        "Ref",
        "Note",
        "Line",
        "Unit",
    ] {
        let value = hostile(tag);
        assert!(
            page.html.contains(&escaped(&value)),
            "{tag} is not printed, or not escaped: {}",
            page.html
        );
        assert!(
            !page.html.contains(&value),
            "{tag} reached the page unescaped: {}",
            page.html
        );
    }

    // The one shape a raw value could take that matters most: markup.
    assert!(
        !page.html.contains("<b>"),
        "a tag from a record survived: {}",
        page.html
    );
    // The page is still self-contained after all that.
    for forbidden in ["<script", "src=", "@import"] {
        assert!(!page.html.contains(forbidden), "found {forbidden}");
    }
}
