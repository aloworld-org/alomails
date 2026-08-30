//! Finance and Billing answering the same period with the same figure (A10.2),
//! on the wire: the real router, the real store, real documents raised through
//! the `/billing/` routes.
//!
//! The defect this suite pins was found by the A9.1 evaluation run. Documents
//! have only been posted to the journal since the day the document paths were
//! wired to the posting rules; a tenant who invoiced before that upgrade holds
//! issued documents the books do not know about. `ledger_summary` reads the
//! journal and `billing_totals` reads the documents, so the two agents answered
//! the same question about the same tenant in the same minute with `0.00` and
//! the true figure — and neither said why.
//!
//! The pre-wiring state is manufactured with test-only surgery on the journal,
//! because the product itself can no longer produce it: every route that issues,
//! pays or credits a document now books it as it goes.
//!
//! Four things are held here. The reading never reports the journal's figure as
//! the whole truth while documents are missing from it; the repair puts them in,
//! at their own dates, and says what it posted; running the repair twice posts
//! nothing the second time; and afterwards the two agents agree, figure for
//! figure. Beside them: a refusal names the document it belongs to in the
//! store's own words and burns nothing, and one tenant's repair never reaches
//! another tenant's documents.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use serde_json::{Value, json};
use sqlx::postgres::PgPoolOptions;
use time::OffsetDateTime;

use crate::common::{Harness, database_url, harness, seed_default_chart, send};

/// Gross of the first invoice: two days at 100.00 plus 21% VAT.
const FIRST_GROSS: i64 = 24_200;
/// Gross of the second: five days at 100.00 plus 21% VAT.
const SECOND_GROSS: i64 = 60_500;
/// What arrived against the first.
const PAID: i64 = 10_000;

async fn post(app: &Router, token: &str, uri: &str, body: Value) -> (StatusCode, Value) {
    let req = Request::builder()
        .method("POST")
        .uri(uri)
        .header("content-type", "application/json")
        .header("authorization", format!("Bearer {token}"))
        .body(Body::from(body.to_string()))
        .unwrap();
    send(app, req).await
}

/// Runs one verb the way an approved proposal runs it, and answers its result.
async fn run(h: &Harness, tool: &str, args: Value) -> Value {
    let (status, body) = post(
        &h.app,
        &h.token,
        "/ai/agent/execute",
        json!({ "tool": tool, "args": args }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{tool}: {body}");
    body["result"].clone()
}

/// A tenant whose books are wired, with a customer to bill — the admin gate is
/// the Finance one, because every verb here reads the whole tenant's books.
async fn a_tenant(tag: &str) -> (Harness, String) {
    let h = harness(tag).await;
    h.ts.set_admin(&h.user, true).await.unwrap();
    seed_default_chart(&h.acc).await;
    let (status, body) = post(
        &h.app,
        &h.token,
        "/billing/customers",
        json!({ "name": "Northstar Foods BV", "addressLine1": "Demo Street 1",
                "postalCode": "1011 AB", "city": "Amsterdam", "country": "NL",
                "paymentTermsDays": 30, "currency": "EUR" }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let customer = body["customer"]["id"].as_str().unwrap().to_owned();
    (h, customer)
}

/// An issued invoice of `days` × 100.00 at 21%, and its id.
async fn an_issued_invoice(h: &Harness, customer: &str, days: i64) -> String {
    let (status, body) = post(
        &h.app,
        &h.token,
        "/billing/invoices",
        json!({ "customerId": customer,
                "lines": [{ "description": "Consulting", "qtyMilli": days * 1_000,
                            "unitPriceCents": 10_000, "vatRateBp": 2_100 }] }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let id = body["invoice"]["id"].as_str().unwrap().to_owned();
    let (status, body) = post(
        &h.app,
        &h.token,
        &format!("/billing/invoices/{id}/issue"),
        json!({}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    id
}

/// An issued credit note against `invoice` — the whole of it, mirrored.
async fn an_issued_credit_note(h: &Harness, invoice: &str) -> String {
    let (status, body) = post(
        &h.app,
        &h.token,
        &format!("/billing/invoices/{invoice}/credit-note"),
        json!({}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let id = body["invoice"]["id"].as_str().unwrap().to_owned();
    let (status, body) = post(
        &h.app,
        &h.token,
        &format!("/billing/invoices/{id}/issue"),
        json!({}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    id
}

/// Money against a document.
async fn a_payment(h: &Harness, invoice: &str, cents: i64) {
    let (status, body) = post(
        &h.app,
        &h.token,
        &format!("/billing/invoices/{invoice}/payments"),
        json!({ "amountCents": cents, "method": "transfer" }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
}

/// **The surgery.** Empties this tenant's journal, leaving the documents where
/// they are — the shape of a tenant who invoiced before the books started
/// recording documents, which no route can produce any more.
async fn forget_the_journal(h: &Harness) {
    let pool = PgPoolOptions::new()
        .max_connections(1)
        .connect(&database_url())
        .await
        .unwrap();
    for table in ["fin_postings", "fin_entries"] {
        sqlx::query(&format!("DELETE FROM {table} WHERE tenant_id = $1"))
            .bind(h.tenant.as_str())
            .execute(&pool)
            .await
            .unwrap();
    }
}

/// Two invoices, a part-payment on the first and a credit note against it —
/// all issued through the routes, then removed from the books.
async fn a_tenant_that_invoiced_before_the_books(tag: &str) -> Harness {
    let (h, customer) = a_tenant(tag).await;
    let first = an_issued_invoice(&h, &customer, 2).await;
    a_payment(&h, &first, PAID).await;
    an_issued_credit_note(&h, &first).await;
    an_issued_invoice(&h, &customer, 5).await;
    forget_the_journal(&h).await;
    h
}

/// The whole of the defect and the whole of the repair, in one walk: the books
/// read zero, the reading says why, the repair puts the documents in, and the
/// two agents then answer the period with the same figures.
#[tokio::test]
async fn the_books_catch_up_with_the_documents_and_the_two_agents_agree() {
    let h = a_tenant_that_invoiced_before_the_books("fin-books-agree").await;

    // Before. The journal is empty, and the reading says so rather than
    // reporting 0.00 as the truth about the period.
    let before = run(&h, "ledger_summary", json!({})).await;
    assert_eq!(before["invoicedCents"], 0);
    assert_eq!(before["paidCents"], 0);
    assert_eq!(before["entryCount"], 0);
    let documents = &before["documents"];
    assert_eq!(documents["compared"], true);
    assert_eq!(documents["documentCount"], 3);
    assert_eq!(documents["unpostedCount"], 3);
    assert_eq!(
        documents["unpostedCents"],
        FIRST_GROSS + SECOND_GROSS - FIRST_GROSS,
        "the credit note takes its original back off again: {documents}"
    );
    assert_eq!(documents["unpostedUnconvertedCount"], 0);
    let note = documents["note"].as_str().unwrap();
    assert!(
        note.contains("3 of these documents are not in the books"),
        "{note}"
    );
    assert!(note.contains("605.00 EUR"), "{note}");
    assert!(note.contains("post_missing_documents"), "{note}");
    // The three are named, oldest first and the credit note last, so a person
    // reading the answer knows which documents are meant.
    let named = documents["unposted"].as_array().unwrap();
    assert_eq!(named.len(), 3);
    assert_eq!(named[0]["creditNote"], false);
    assert_eq!(named[2]["creditNote"], true);

    // …and the disagreement the evaluation found is real: Billing's own totals
    // over the same period are not zero.
    let totals = run(&h, "billing_totals", json!({})).await;
    assert_eq!(totals["invoicedGrossCents"], FIRST_GROSS + SECOND_GROSS);
    assert_ne!(totals["invoicedGrossCents"], before["invoicedCents"]);

    // The repair. Three documents and one settlement, and it says which.
    let posted = run(&h, "post_missing_documents", json!({})).await;
    assert_eq!(posted["kind"], "booksBackfill");
    assert_eq!(posted["documentCount"], 3);
    assert_eq!(posted["postedDocumentCount"], 3);
    assert_eq!(posted["postedSettlementCount"], 1);
    assert_eq!(posted["alreadyInTheBooksCount"], 0);
    assert_eq!(posted["postedCents"], SECOND_GROSS);
    assert_eq!(posted["postedDisplay"], "605.00 EUR");
    assert_eq!(posted["refusedCount"], 0);

    // After. The books hold the documents, and the two agents agree — figure
    // for figure, over the same period, in the same currency.
    let after = run(&h, "ledger_summary", json!({})).await;
    assert_eq!(after["invoicedCents"], totals["invoicedGrossCents"]);
    assert_eq!(after["paidCents"], totals["paidCents"]);
    assert_eq!(after["paidCents"], PAID);
    assert_eq!(
        after["creditNotedCents"].as_i64().unwrap(),
        -totals["creditNotesGrossCents"].as_i64().unwrap(),
        "the same credit note, signed as each surface signs it"
    );
    assert_eq!(after["documents"]["unpostedCount"], 0);
    assert!(
        after["documents"]["note"].is_null(),
        "nothing is missing, so nothing is said: {after}"
    );
}

/// Running the repair on books that are already whole posts nothing and says
/// so. Its idempotency is the journal's own uniqueness key, not a flag this
/// path keeps, so a second run cannot double a single posting.
#[tokio::test]
async fn a_second_repair_posts_nothing_and_counts_what_was_already_there() {
    let h = a_tenant_that_invoiced_before_the_books("fin-books-again").await;

    let first = run(&h, "post_missing_documents", json!({})).await;
    assert_eq!(first["postedDocumentCount"], 3);
    assert_eq!(first["postedSettlementCount"], 1);

    let again = run(&h, "post_missing_documents", json!({})).await;
    assert_eq!(again["documentCount"], 3);
    assert_eq!(again["alreadyInTheBooksCount"], 3);
    assert_eq!(again["postedDocumentCount"], 0);
    assert_eq!(again["postedSettlementCount"], 0);
    assert_eq!(again["postedCents"], 0);
    assert_eq!(again["refusedCount"], 0);

    // And the books did not move under the second run.
    let after = run(&h, "ledger_summary", json!({})).await;
    assert_eq!(after["invoicedCents"], FIRST_GROSS + SECOND_GROSS);
    assert_eq!(after["documents"]["unpostedCount"], 0);
}

/// A document the books refuse is named, with the store's own sentence, and
/// nothing else is disturbed. The realistic case: the period the old documents
/// belong to has since been closed, and a repair may not write into it.
#[tokio::test]
async fn a_document_the_books_refuse_is_named_and_nothing_is_posted_behind_it() {
    let h = a_tenant_that_invoiced_before_the_books("fin-books-closed").await;
    let year = OffsetDateTime::now_utc().year();

    let (status, body) = post(
        &h.app,
        &h.token,
        "/finance/periods",
        json!({ "fromDate": format!("{year}-01-01"), "toDate": format!("{year}-12-31") }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let period = body["period"]["id"].as_str().unwrap().to_owned();
    let (status, body) = post(
        &h.app,
        &h.token,
        &format!("/finance/periods/{period}/close"),
        json!({ "note": "filed" }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");

    let refused = run(&h, "post_missing_documents", json!({})).await;
    assert_eq!(refused["postedDocumentCount"], 0);
    assert_eq!(refused["postedSettlementCount"], 0);
    assert_eq!(refused["refusedCount"], 3, "{refused}");
    let first = &refused["refused"][0];
    assert!(
        first["document"]["number"].as_str().unwrap().contains("-0"),
        "a refusal names the document: {first}"
    );
    let reason = first["reason"].as_str().unwrap();
    assert!(
        reason.contains("closed"),
        "the store's own sentence, not ours: {reason}"
    );
    // Nothing was booked, so the reading still reports the whole difference.
    let still = run(&h, "ledger_summary", json!({})).await;
    assert_eq!(still["invoicedCents"], 0);
    assert_eq!(still["documents"]["unpostedCount"], 3);

    // Reopened, the same repair goes through — the refusal cost nothing.
    let (status, body) = post(
        &h.app,
        &h.token,
        &format!("/finance/periods/{period}/reopen"),
        json!({ "note": "the documents raised before the books were opened are going in" }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let posted = run(&h, "post_missing_documents", json!({})).await;
    assert_eq!(posted["postedDocumentCount"], 3);
    assert_eq!(posted["refusedCount"], 0);
}

/// **Tenant isolation.** The repair walks the asker's own documents and posts
/// into the asker's own books. Another tenant's unposted documents are not
/// counted, not posted, and not named — the wrong-tenant proof for a path whose
/// whole job is to write into a journal.
#[tokio::test]
async fn one_tenants_repair_never_reaches_another_tenants_documents() {
    let ours = a_tenant_that_invoiced_before_the_books("fin-books-ours").await;
    let theirs = a_tenant_that_invoiced_before_the_books("fin-books-theirs").await;

    let posted = run(&ours, "post_missing_documents", json!({})).await;
    assert_eq!(
        posted["documentCount"], 3,
        "three documents, not six: {posted}"
    );
    assert_eq!(posted["postedDocumentCount"], 3);

    // Their books are untouched, and their reading still reports the gap.
    let theirs_after = run(&theirs, "ledger_summary", json!({})).await;
    assert_eq!(theirs_after["invoicedCents"], 0);
    assert_eq!(theirs_after["entryCount"], 0);
    assert_eq!(theirs_after["documents"]["unpostedCount"], 3);

    // …until they run it themselves, on their own.
    let theirs_posted = run(&theirs, "post_missing_documents", json!({})).await;
    assert_eq!(theirs_posted["postedDocumentCount"], 3);
    let ours_after = run(&ours, "ledger_summary", json!({})).await;
    assert_eq!(ours_after["invoicedCents"], FIRST_GROSS + SECOND_GROSS);
}

/// The books are not open to whoever asks. The repair writes the journal, so it
/// is gated exactly as the Finance screens are: an ordinary member of the tenant
/// is refused before a single document is read.
#[tokio::test]
async fn a_member_who_may_not_see_the_books_may_not_repair_them() {
    let h = a_tenant_that_invoiced_before_the_books("fin-books-gate").await;
    h.ts.set_admin(&h.user, false).await.unwrap();

    let (status, body) = post(
        &h.app,
        &h.token,
        "/ai/agent/execute",
        json!({ "tool": "post_missing_documents", "args": {} }),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "{body}");

    // And the books are exactly as they were.
    h.ts.set_admin(&h.user, true).await.unwrap();
    let reading = run(&h, "ledger_summary", json!({})).await;
    assert_eq!(reading["entryCount"], 0);
    assert_eq!(reading["documents"]["unpostedCount"], 3);
}
