//! Receiving an e-invoice over HTTP (B1.24): `POST /billing/bills/import` and
//! the approval routes over it, driven through the real router over a real
//! Postgres.
//!
//! Three things make this route different from every other billing write, and
//! this suite is those three:
//!
//! - **The body is somebody else's file.** So the refusals are the interesting
//!   half: a PDF, a JSON blob, a document whose totals do not add up, a line we
//!   cannot represent — each answered with the business term or the standard's
//!   rule identifier, so the person holding the file can tell the supplier what
//!   is wrong with it.
//! - **It is the inverse of what we write.** Every one of our own golden
//!   e-invoices (B1.22, B1.23) is imported back here and checked figure by
//!   figure against what the invoice they were rendered from was worth. That is
//!   the closest offline equivalent of "the official samples import": these
//!   files are the two syntaxes in law, produced by the standard's rules, and
//!   nothing in the reader shares code with the writer that made them.
//! - **A decision is final.** The approval door is tested from both sides,
//!   including the `409` that a second decision gets.
//!
//! And the mandatory question: can tenant B reach tenant A's bill, and does the
//! refusal for A's id differ in any byte from the refusal for an id that never
//! existed.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use serde_json::{Value, json};

use crate::common::{harness, send};

// ---- request helpers ---------------------------------------------------------

/// A request carrying an uploaded file as its body.
fn upload(uri: &str, token: Option<&str>, body: &[u8]) -> Request<Body> {
    let mut req = Request::builder()
        .method("POST")
        .uri(uri)
        .header("content-type", "application/xml");
    if let Some(token) = token {
        req = req.header("authorization", format!("Bearer {token}"));
    }
    req.body(Body::from(body.to_vec())).unwrap()
}

/// A request with no body worth naming — a `GET`, a `DELETE`, or one of the
/// decision `POST`s (which deliberately carry nothing).
fn plain(method: &str, uri: &str, token: Option<&str>) -> Request<Body> {
    let mut req = Request::builder().method(method).uri(uri);
    if let Some(token) = token {
        req = req.header("authorization", format!("Bearer {token}"));
    }
    req.body(Body::empty()).unwrap()
}

async fn import(app: &Router, token: &str, file: &[u8]) -> (StatusCode, Value) {
    send(app, upload("/billing/bills/import", Some(token), file)).await
}

async fn get(app: &Router, token: &str, uri: &str) -> (StatusCode, Value) {
    send(app, plain("GET", uri, Some(token))).await
}

async fn act(app: &Router, token: &str, uri: &str) -> (StatusCode, Value) {
    send(app, plain("POST", uri, Some(token))).await
}

/// The detail of a refusal, which is the whole point of this surface.
fn detail(body: &Value) -> String {
    body["detail"].as_str().unwrap_or_default().to_owned()
}

fn imported(result: (StatusCode, Value)) -> Value {
    let (status, body) = result;
    assert_eq!(status, StatusCode::OK, "import failed: {body}");
    body["bill"].clone()
}

/// One of our own e-invoices, as rendered by B1.22 / B1.23.
fn golden(name: &str) -> Vec<u8> {
    let path = format!("{}/tests/golden/{name}", env!("CARGO_MANIFEST_DIR"));
    std::fs::read(&path).unwrap_or_else(|e| panic!("read {path}: {e}"))
}

/// A supplier's invoice with two lines: 8 h at €125.00 and 240 km at €0.42.
/// Line total €1100.80, VAT €231.17 at 21 %, gross €1331.97 — hand-computed.
fn supplier_invoice(number: &str) -> String {
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
        <ram:Name>Lieferant GmbH</ram:Name>
        <ram:PostalTradeAddress><ram:PostcodeCode>10115</ram:PostcodeCode><ram:LineOne>Hauptstraße 5</ram:LineOne><ram:CityName>Berlin</ram:CityName><ram:CountryID>DE</ram:CountryID></ram:PostalTradeAddress>
        <ram:SpecifiedTaxRegistration><ram:ID schemeID="VA">DE811907980</ram:ID></ram:SpecifiedTaxRegistration>
      </ram:SellerTradeParty>
    </ram:ApplicableHeaderTradeAgreement>
    <ram:ApplicableHeaderTradeSettlement>
      <ram:InvoiceCurrencyCode>EUR</ram:InvoiceCurrencyCode>
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

#[tokio::test]
async fn the_arc_from_an_uploaded_file_to_an_approved_bill() {
    let h = harness("bills-arc").await;

    // No token, no bills — on every verb of the surface.
    for request in [
        upload("/billing/bills/import", None, b"<x/>"),
        plain("GET", "/billing/bills", None),
        plain("GET", "/billing/bills/x", None),
        plain("POST", "/billing/bills/x/approve", None),
        plain("POST", "/billing/bills/x/reject", None),
        plain("DELETE", "/billing/bills/x", None),
    ] {
        let (status, _) = send(&h.app, request).await;
        assert_eq!(status, StatusCode::UNAUTHORIZED);
    }

    let bill = imported(import(&h.app, &h.token, supplier_invoice("R-2026-77").as_bytes()).await);
    let id = bill["id"].as_str().unwrap().to_owned();
    assert_eq!(bill["status"], "received");
    assert_eq!(bill["sourceSyntax"], "cii");
    assert_eq!(bill["number"], "R-2026-77", "their number");
    assert_eq!(bill["issueDate"], "2026-08-07");
    assert_eq!(bill["dueDate"], "2026-09-06");
    assert_eq!(bill["supplier"]["name"], "Lieferant GmbH");
    assert_eq!(bill["supplier"]["vatId"], "DE811907980");
    assert_eq!(bill["totals"]["lineTotalCents"], json!(110_080));
    assert_eq!(bill["totals"]["taxTotalCents"], json!(23_117));
    assert_eq!(bill["totals"]["payableCents"], json!(133_197));
    // What the supplier states and what their lines add up to, both reported
    // and agreeing — which is what the import refuses to store without.
    assert_eq!(bill["computed"]["netCents"], json!(110_080));
    assert_eq!(bill["computed"]["vatCents"], json!(23_117));
    assert_eq!(bill["computed"]["grossCents"], json!(133_197));
    assert_eq!(bill["lines"].as_array().map(Vec::len), Some(2));
    assert_eq!(bill["lines"][0]["unit"], "hour");
    assert_eq!(bill["lines"][1]["qtyMilli"], json!(240_000));

    // The approval queue.
    let (status, body) = get(&h.app, &h.token, "/billing/bills?status=received").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["bills"].as_array().map(Vec::len), Some(1));
    let (_, approved_list) = get(&h.app, &h.token, "/billing/bills?status=approved").await;
    assert_eq!(approved_list["bills"].as_array().map(Vec::len), Some(0));
    let (status, body) = get(&h.app, &h.token, "/billing/bills?status=whenever").await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert!(detail(&body).contains("received, approved or rejected"));

    // One bill, by id.
    let (status, body) = get(&h.app, &h.token, &format!("/billing/bills/{id}")).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["bill"]["id"], json!(id));

    // The same file again is one bill, not two.
    let (status, body) = import(&h.app, &h.token, supplier_invoice("R-2026-77").as_bytes()).await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert!(detail(&body).contains("already been imported"), "{body}");

    // The decision, and its finality.
    let (status, body) = act(&h.app, &h.token, &format!("/billing/bills/{id}/approve")).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["bill"]["status"], "approved");
    assert!(body["bill"]["decidedBy"].is_string(), "who decided");
    assert!(body["bill"]["decidedAt"].is_string(), "and when");
    for again in ["approve", "reject"] {
        let (status, body) = act(&h.app, &h.token, &format!("/billing/bills/{id}/{again}")).await;
        assert_eq!(status, StatusCode::CONFLICT);
        assert!(detail(&body).contains("already been approved"), "{body}");
    }
    let (status, body) = send(
        &h.app,
        plain("DELETE", &format!("/billing/bills/{id}"), Some(&h.token)),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert!(detail(&body).contains("cannot be deleted"), "{body}");

    // The undo that does exist, for a file that should not have been imported.
    let mistake =
        imported(import(&h.app, &h.token, supplier_invoice("R-2026-78").as_bytes()).await);
    let mistake = mistake["id"].as_str().unwrap().to_owned();
    let (status, _) = send(
        &h.app,
        plain(
            "DELETE",
            &format!("/billing/bills/{mistake}"),
            Some(&h.token),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    let (status, body) = get(&h.app, &h.token, &format!("/billing/bills/{mistake}")).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(detail(&body), "no such bill");
}

#[tokio::test]
async fn a_file_that_is_not_a_readable_e_invoice_is_refused_with_the_rule_it_breaks() {
    let h = harness("bills-refuse").await;

    // A PDF: the obvious thing to try, and answered for what it is.
    let (status, body) = import(&h.app, &h.token, b"%PDF-1.7\nnot xml").await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert!(detail(&body).contains("PDF"), "{body}");
    assert!(detail(&body).contains("factur-x.xml"), "{body}");

    // Not XML, and XML that is not an e-invoice.
    let (status, body) = import(&h.app, &h.token, b"{\"invoice\":true}").await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert!(detail(&body).contains("XML"), "{body}");
    let (status, body) = import(&h.app, &h.token, b"<Order><ID>1</ID></Order>").await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert!(detail(&body).contains("CrossIndustryInvoice"), "{body}");

    // A document type declaration is refused unread, before any expansion.
    let bomb = br#"<?xml version="1.0"?><!DOCTYPE lolz [<!ENTITY lol "lol">]><Doc>&lol;</Doc>"#;
    let (status, body) = import(&h.app, &h.token, bomb).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert!(
        detail(&body).contains("document type declaration"),
        "{body}"
    );

    // Totals that do not add up — the one that would otherwise be paid wrong.
    let tampered = supplier_invoice("R-2026-77").replace(
        "<ram:GrandTotalAmount>1331.97</ram:GrandTotalAmount>",
        "<ram:GrandTotalAmount>1391.97</ram:GrandTotalAmount>",
    );
    let (status, body) = import(&h.app, &h.token, tampered.as_bytes()).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert!(detail(&body).contains("BR-CO-15"), "{body}");

    // A line whose amount is not quantity × price.
    let discounted = supplier_invoice("R-2026-77").replace(
        "<ram:LineTotalAmount>1000.00</ram:LineTotalAmount>",
        "<ram:LineTotalAmount>900.00</ram:LineTotalAmount>",
    );
    let (status, body) = import(&h.app, &h.token, discounted.as_bytes()).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert!(
        detail(&body).contains("line 1") && detail(&body).contains("BT-131"),
        "{body}"
    );

    // A reverse-charge line: 0 % that means we owe the VAT, refused by name
    // rather than stored as a zero-rated one.
    let reverse = supplier_invoice("R-2026-77").replace(
        "<ram:CategoryCode>S</ram:CategoryCode>",
        "<ram:CategoryCode>AE</ram:CategoryCode>",
    );
    let (status, body) = import(&h.app, &h.token, reverse.as_bytes()).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert!(detail(&body).contains("AE"), "{body}");

    // Nothing was written by any of them.
    let (_, body) = get(&h.app, &h.token, "/billing/bills").await;
    assert_eq!(body["bills"].as_array().map(Vec::len), Some(0));
}

#[tokio::test]
async fn every_e_invoice_we_write_imports_back_to_the_figures_it_was_written_from() {
    // The closest offline equivalent of "the official samples import": our own
    // renderings of four invoices, in both syntaxes in law, read back by a
    // reader that shares no code with the writer. The expected figures are the
    // ones the invoices those files were rendered from were worth (B1.22,
    // B1.23) — a credit note in **ledger** direction, negative, because that is
    // how our ledger holds one.
    let expected: [(&str, &str, bool, i64, i64, i64); 4] = [
        (
            "standard",
            "INV-2026-00001",
            false,
            197_580,
            41_492,
            239_072,
        ),
        (
            "mixed-rates",
            "INV-2026-00002",
            false,
            368_400,
            69_306,
            437_706,
        ),
        (
            "credit-note",
            "INV-2026-00003",
            true,
            -187_500,
            -39_375,
            -226_875,
        ),
        (
            "foreign-currency",
            "INV-2026-00004",
            false,
            120_000,
            25_200,
            145_200,
        ),
    ];

    // One tenant per syntax: the CII and the UBL rendering of one invoice are
    // the *same document*, so importing both into one tenant is exactly the
    // duplicate the store refuses — which is itself the right behaviour.
    let cii = harness("bills-golden-cii").await;
    let ubl = harness("bills-golden-ubl").await;

    for (name, number, credit_note, net, vat, gross) in expected {
        for (h, file, syntax) in [
            (
                &cii,
                if name == "credit-note" {
                    "credit-note.xml".to_owned()
                } else {
                    format!("invoice-{name}.xml")
                },
                "cii",
            ),
            (&ubl, format!("xrechnung-{name}.xml"), "ubl"),
        ] {
            let bill = imported(import(&h.app, &h.token, &golden(&file)).await);
            assert_eq!(bill["sourceSyntax"], syntax, "{file}");
            assert_eq!(bill["number"], number, "{file}");
            assert_eq!(bill["creditNote"], json!(credit_note), "{file}");
            assert_eq!(
                bill["currency"],
                if name == "foreign-currency" {
                    "USD"
                } else {
                    "EUR"
                }
            );
            // The supplier is us, since we wrote these — read back out of the
            // file rather than assumed.
            assert_eq!(bill["supplier"]["name"], "Alo Werkplaats B.V.", "{file}");
            assert_eq!(bill["supplier"]["vatId"], "NL812345678B01", "{file}");
            assert_eq!(bill["supplier"]["country"], "NL", "{file}");
            // The figures, in both directions: what the file states, and what
            // its lines add up to under our own arithmetic.
            assert_eq!(bill["totals"]["lineTotalCents"], json!(net), "{file}");
            assert_eq!(bill["totals"]["taxTotalCents"], json!(vat), "{file}");
            assert_eq!(bill["totals"]["payableCents"], json!(gross), "{file}");
            assert_eq!(bill["computed"]["netCents"], json!(net), "{file}");
            assert_eq!(bill["computed"]["vatCents"], json!(vat), "{file}");
            assert_eq!(bill["computed"]["grossCents"], json!(gross), "{file}");
        }
    }

    // The mixed-rates document is the interesting one: three rates read back
    // as three, in the same breakdown our own totals produce.
    let (_, body) = get(&cii.app, &cii.token, "/billing/bills").await;
    let mixed = body["bills"]
        .as_array()
        .unwrap()
        .iter()
        .find(|bill| bill["number"] == "INV-2026-00002")
        .unwrap()
        .clone();
    let (_, document) = get(
        &cii.app,
        &cii.token,
        &format!("/billing/bills/{}", mixed["id"].as_str().unwrap()),
    )
    .await;
    let rates = document["bill"]["computed"]["vatByRate"]
        .as_array()
        .unwrap();
    assert_eq!(rates.len(), 3);
    assert_eq!(rates[0]["rateBp"], json!(0));
    assert_eq!(rates[0]["vatCents"], json!(0));
    assert_eq!(rates[1]["rateBp"], json!(900));
    assert_eq!(rates[1]["vatCents"], json!(2_106));
    assert_eq!(rates[2]["rateBp"], json!(2100));
    assert_eq!(rates[2]["vatCents"], json!(67_200));
}

#[tokio::test]
async fn a_neighbours_bill_is_not_reachable_and_the_refusal_gives_nothing_away() {
    let a = harness("bills-tenant-a").await;
    let b = harness("bills-tenant-b").await;

    // B's document names B's supplier — distinctive enough that a leak would
    // be visible anywhere in A's answers.
    let b_file = supplier_invoice("R-2026-77")
        .replace("Lieferant GmbH", "Nachbar Sondermaschinenbau AG")
        .replace("DE811907980", "DE136695976");
    let b_bill = imported(import(&b.app, &b.token, b_file.as_bytes()).await);
    let b_id = b_bill["id"].as_str().unwrap().to_owned();

    // A sees nothing of it, and the refusal for B's id is byte-identical to
    // the refusal for an id that never existed.
    let (ghost_status, ghost_body) = get(&a.app, &a.token, "/billing/bills/never-existed").await;
    let (status, body) = get(&a.app, &a.token, &format!("/billing/bills/{b_id}")).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(ghost_status, StatusCode::NOT_FOUND);
    assert_eq!(body.to_string(), ghost_body.to_string());

    for uri in [
        format!("/billing/bills/{b_id}/approve"),
        format!("/billing/bills/{b_id}/reject"),
    ] {
        let (status, body) = act(&a.app, &a.token, &uri).await;
        assert_eq!(status, StatusCode::NOT_FOUND, "{body}");
        assert!(!body.to_string().contains("Nachbar"), "{body}");
    }
    let (status, body) = send(
        &a.app,
        plain("DELETE", &format!("/billing/bills/{b_id}"), Some(&a.token)),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert!(!body.to_string().contains("Nachbar"));

    // A's list is empty, in every filter, and mentions nothing of B's.
    for uri in [
        "/billing/bills",
        "/billing/bills?status=received",
        "/billing/bills?status=approved",
    ] {
        let (status, body) = get(&a.app, &a.token, uri).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["bills"].as_array().map(Vec::len), Some(0), "{uri}");
        for leak in ["Nachbar", "DE136695976", "R-2026-77"] {
            assert!(!body.to_string().contains(leak), "{uri} leaked {leak}");
        }
    }

    // B's bill is untouched by everything A tried.
    let (status, body) = get(&b.app, &b.token, &format!("/billing/bills/{b_id}")).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["bill"]["status"], "received");
    assert_eq!(
        body["bill"]["supplier"]["name"],
        "Nachbar Sondermaschinenbau AG"
    );
}
