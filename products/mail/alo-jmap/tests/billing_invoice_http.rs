//! The `/billing/invoices` HTTP surface (B1.10), driven through the real
//! router over a real Postgres.
//!
//! `alo-store`'s own suites prove the document works; what this suite is for is
//! the **edge**: that the arc a bookkeeper actually walks — raise a draft, edit
//! it, issue it, credit it — comes back over the wire with the numbers and the
//! status codes `docs/design/billing.md` publishes; that money on every
//! response is the server's arithmetic and never a client's; that a frozen
//! document refuses every write verb; and that another tenant's document is
//! invisible and untouchable on all seven routes, answering exactly as an id
//! that never existed.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use crate::common;

use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use serde_json::{Value, json};
use sqlx::postgres::PgPoolOptions;
use time::format_description::well_known::Iso8601;
use time::{Duration, OffsetDateTime};

use crate::common::{database_url, harness, send};

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

async fn delete(app: &Router, token: &str, uri: &str) -> (StatusCode, Value) {
    send(app, with_json("DELETE", uri, Some(token), json!({}))).await
}

/// The id of a created resource, or a panic naming the status that came back
/// instead — a failed create otherwise shows up as a confusing later failure.
fn created_id(kind: &str, (status, body): (StatusCode, Value)) -> String {
    assert_eq!(status, StatusCode::OK, "create failed: {body}");
    body[kind]["id"]
        .as_str()
        .unwrap_or_else(|| panic!("no {kind} id in {body}"))
        .to_owned()
}

/// A customer on 14-day terms in euro — the party every document here is
/// raised for.
async fn a_customer(app: &Router, token: &str) -> String {
    created_id(
        "customer",
        post(
            app,
            token,
            "/billing/customers",
            json!({
                "name": "Acme GmbH",
                "addressLine1": "Hauptstraße 1",
                "postalCode": "10115",
                "city": "Berlin",
                "country": "DE",
                "vatId": "DE811907980",
                "paymentTermsDays": 14,
                "currency": "EUR",
            }),
        )
        .await,
    )
}

/// Three lines across two VAT rates, with a quantity that is not a whole unit
/// and a price whose VAT lands on a half cent — so the totals below can only
/// come out right if the server rounds once per rate, as the design note says.
fn three_lines() -> Value {
    json!([
        { "description": "Consulting", "unit": "hour", "qtyMilli": 1_500,
          "unitPriceCents": 12_500, "vatRateBp": 2_100 },
        { "description": "Support call", "unit": "piece", "qtyMilli": 3_000,
          "unitPriceCents": 999, "vatRateBp": 2_100 },
        { "description": "Printed manual", "unit": "piece", "qtyMilli": 1_000,
          "unitPriceCents": 5_000, "vatRateBp": 700 },
    ])
}

fn today() -> time::Date {
    OffsetDateTime::now_utc().date()
}

fn as_day(d: time::Date) -> String {
    d.format(&Iso8601::DATE).unwrap()
}

fn ids(body: &Value, key: &str) -> Vec<String> {
    body[key]
        .as_array()
        .unwrap_or_else(|| panic!("no {key} array in {body}"))
        .iter()
        .map(|i| i["id"].as_str().unwrap_or_default().to_owned())
        .collect()
}

// ---- the arc (the item's done-when) -----------------------------------------

#[tokio::test]
async fn the_draft_to_issue_to_credit_arc_runs_on_the_wire() {
    let h = harness("bill-inv-arc").await;
    common::seed_default_chart(&h.acc).await;
    let customer = a_customer(&h.app, &h.token).await;

    // --- raise a draft, header and lines in one body -------------------------
    let (status, body) = post(
        &h.app,
        &h.token,
        "/billing/invoices",
        json!({ "customerId": customer, "reference": "PO-77", "lines": three_lines() }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let invoice = &body["invoice"];
    let id = invoice["id"].as_str().unwrap().to_owned();
    assert_eq!(invoice["status"], "draft");
    assert_eq!(invoice["reference"], "PO-77");
    // The customer's own defaults were resolved and snapshotted on the
    // document, not left dangling as a reference to a record that may change.
    assert_eq!(invoice["currency"], "EUR");
    assert_eq!(invoice["paymentTermsDays"], 14);
    // A draft carries no number and no dates — that is what makes it a draft.
    assert_eq!(invoice["number"], Value::Null);
    assert_eq!(invoice["issueDate"], Value::Null);
    assert_eq!(invoice["dueDate"], Value::Null);
    assert_eq!(invoice["overdue"], false);
    assert_eq!(invoice["creditNote"], false);
    assert_eq!(invoice["creditsInvoiceId"], Value::Null);

    // The money is the server's, rounded once per rate: nets 18 750 + 2 997 at
    // 21 % (VAT 4 566.87 → 4 567) and 5 000 at 7 % (VAT 350).
    let totals = &invoice["totals"];
    assert_eq!(totals["netCents"], 26_747);
    assert_eq!(totals["vatCents"], 4_917);
    assert_eq!(totals["grossCents"], 31_664);
    assert_eq!(
        totals["vatByRate"],
        json!([
            { "rateBp": 700, "netCents": 5_000, "vatCents": 350 },
            { "rateBp": 2_100, "netCents": 21_747, "vatCents": 4_567 },
        ]),
        "the breakdown must be per rate, in rate order"
    );
    // Lines come back in the order they were sent, each with its own net.
    let lines = invoice["lines"].as_array().unwrap();
    assert_eq!(lines.len(), 3);
    assert_eq!(lines[0]["description"], "Consulting");
    assert_eq!(lines[0]["netCents"], 18_750);
    assert_eq!(lines[1]["netCents"], 2_997);
    assert_eq!(lines[2]["netCents"], 5_000);
    // There is no per-line VAT anywhere: it is rounded at the rate subtotal,
    // so a per-line column would not add up to the document's own.
    assert!(lines[0].get("vatCents").is_none(), "{}", lines[0]);

    // --- edit it: a header-only PATCH leaves the lines alone -----------------
    let (status, body) = patch(
        &h.app,
        &h.token,
        &format!("/billing/invoices/{id}"),
        json!({ "note": "Zahlbar innerhalb von 14 Tagen" }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["invoice"]["note"], "Zahlbar innerhalb von 14 Tagen");
    assert_eq!(body["invoice"]["reference"], "PO-77");
    assert_eq!(body["invoice"]["totals"]["grossCents"], 31_664);

    // --- and a lines-only PATCH replaces the whole set, in order -------------
    let (status, body) = patch(
        &h.app,
        &h.token,
        &format!("/billing/invoices/{id}"),
        json!({ "lines": [
            { "description": "Consulting", "unit": "hour", "qtyMilli": 2_000,
              "unitPriceCents": 12_500, "vatRateBp": 2_100 },
            { "description": "Goodwill discount", "qtyMilli": -1_000,
              "unitPriceCents": 2_500, "vatRateBp": 2_100 },
        ] }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let invoice = &body["invoice"];
    assert_eq!(invoice["lines"].as_array().unwrap().len(), 2);
    assert_eq!(invoice["lines"][1]["netCents"], -2_500, "a discount line");
    // 25 000 − 2 500 = 22 500 net at 21 % → VAT 4 725, gross 27 225.
    assert_eq!(invoice["totals"]["netCents"], 22_500);
    assert_eq!(invoice["totals"]["vatCents"], 4_725);
    assert_eq!(invoice["totals"]["grossCents"], 27_225);
    assert_eq!(invoice["note"], "Zahlbar innerhalb von 14 Tagen", "kept");

    // --- issue it: a number, the dates, and the freeze -----------------------
    let (status, body) = post(
        &h.app,
        &h.token,
        &format!("/billing/invoices/{id}/issue"),
        json!({}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let issued = &body["invoice"];
    assert_eq!(issued["status"], "issued");
    assert_eq!(
        issued["number"],
        format!("INV-{}-00001", today().year()),
        "the first document of the tenant's series"
    );
    assert_eq!(issued["issueDate"], as_day(today()));
    assert_eq!(issued["dueDate"], as_day(today() + Duration::days(14)));
    assert_eq!(issued["overdue"], false, "due in a fortnight");
    assert_eq!(issued["totals"]["grossCents"], 27_225, "frozen as it was");

    // Every write verb now refuses, and says which state refused.
    for (method, uri, body) in [
        (
            "PATCH",
            format!("/billing/invoices/{id}"),
            json!({ "reference": "PO-99" }),
        ),
        (
            "PATCH",
            format!("/billing/invoices/{id}"),
            json!({ "lines": [] }),
        ),
        ("DELETE", format!("/billing/invoices/{id}"), json!({})),
        ("POST", format!("/billing/invoices/{id}/issue"), json!({})),
    ] {
        let (status, answer) = send(&h.app, with_json(method, &uri, Some(&h.token), body)).await;
        assert_eq!(status, StatusCode::CONFLICT, "{method} {uri} → {answer}");
        assert!(
            answer["detail"]
                .as_str()
                .unwrap_or_default()
                .contains("issued"),
            "the refusal must name the state: {answer}"
        );
    }
    // …and nothing moved.
    let (_, body) = get(&h.app, &h.token, &format!("/billing/invoices/{id}")).await;
    assert_eq!(body["invoice"]["reference"], "PO-77");
    assert_eq!(body["invoice"]["totals"]["grossCents"], 27_225);
    assert_eq!(body["creditNotes"], json!([]), "nothing credits it yet");

    // --- credit it ------------------------------------------------------------
    let (status, body) = post(
        &h.app,
        &h.token,
        &format!("/billing/invoices/{id}/credit-note"),
        json!({}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let credit = &body["invoice"];
    let credit_id = credit["id"].as_str().unwrap().to_owned();
    assert_ne!(credit_id, id, "the response is the NEW document");
    assert_eq!(
        credit["status"], "draft",
        "the mirror is a starting position"
    );
    assert_eq!(credit["creditNote"], true);
    assert_eq!(credit["creditsInvoiceId"], id);
    assert_eq!(credit["customerId"], customer);
    assert_eq!(credit["currency"], "EUR");
    // The mirror: same lines, same order, quantities negated.
    let credit_lines = credit["lines"].as_array().unwrap();
    assert_eq!(credit_lines.len(), 2);
    assert_eq!(credit_lines[0]["description"], "Consulting");
    assert_eq!(credit_lines[0]["qtyMilli"], -2_000);
    assert_eq!(credit_lines[1]["qtyMilli"], 1_000, "the discount flips too");
    assert_eq!(credit["totals"]["netCents"], -22_500);
    assert_eq!(credit["totals"]["vatCents"], -4_725);
    assert_eq!(credit["totals"]["grossCents"], -27_225);

    // The original now names what credits it, drafts included.
    let (_, body) = get(&h.app, &h.token, &format!("/billing/invoices/{id}")).await;
    assert_eq!(ids(&body, "creditNotes"), vec![credit_id.clone()]);

    // Issuing the credit note draws from the SAME series — an unbroken ledger
    // is one series, not two interleaved ones.
    let (status, body) = post(
        &h.app,
        &h.token,
        &format!("/billing/invoices/{credit_id}/issue"),
        json!({}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(
        body["invoice"]["number"],
        format!("INV-{}-00002", today().year())
    );
    assert_eq!(body["invoice"]["status"], "issued");

    // The ledger closes: the pair sums to zero, every figure of it.
    let (_, original) = get(&h.app, &h.token, &format!("/billing/invoices/{id}")).await;
    let (_, note) = get(&h.app, &h.token, &format!("/billing/invoices/{credit_id}")).await;
    for figure in ["netCents", "vatCents", "grossCents"] {
        let a = original["invoice"]["totals"][figure].as_i64().unwrap();
        let b = note["invoice"]["totals"][figure].as_i64().unwrap();
        assert_eq!(a + b, 0, "{figure} does not close: {a} + {b}");
    }
    assert_eq!(
        note["creditNotes"],
        json!([]),
        "a credit note is not credited"
    );

    // --- the list, and its status filter ------------------------------------
    let draft = created_id(
        "invoice",
        post(
            &h.app,
            &h.token,
            "/billing/invoices",
            json!({ "customerId": customer, "lines": three_lines() }),
        )
        .await,
    );
    let (status, body) = get(&h.app, &h.token, "/billing/invoices").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(ids(&body, "invoices").len(), 3, "{body}");
    // A list entry carries the totals but not the lines — the list surface is
    // not a document dump.
    assert!(body["invoices"][0]["totals"]["grossCents"].is_i64());
    assert!(body["invoices"][0].get("lines").is_none(), "{body}");

    let (_, body) = get(&h.app, &h.token, "/billing/invoices?status=draft").await;
    assert_eq!(ids(&body, "invoices"), vec![draft.clone()]);
    let (_, body) = get(&h.app, &h.token, "/billing/invoices?status=issued").await;
    let listed = ids(&body, "invoices");
    assert_eq!(listed.len(), 2);
    assert!(
        listed.contains(&id) && listed.contains(&credit_id),
        "{listed:?}"
    );
    let (_, body) = get(&h.app, &h.token, "/billing/invoices?status=paid").await;
    assert!(ids(&body, "invoices").is_empty(), "{body}");

    // A draft is discarded, not voided: it never consumed a number.
    let (status, _) = delete(&h.app, &h.token, &format!("/billing/invoices/{draft}")).await;
    assert_eq!(status, StatusCode::OK);
    let (status, _) = get(&h.app, &h.token, &format!("/billing/invoices/{draft}")).await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    // Voiding the issued original keeps its number and stops it being owed;
    // the series is gapless precisely because the cancelled document stays in
    // it.
    let (status, body) = post(
        &h.app,
        &h.token,
        &format!("/billing/invoices/{id}/void"),
        json!({}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["invoice"]["status"], "void");
    assert_eq!(
        body["invoice"]["number"],
        format!("INV-{}-00001", today().year())
    );
    // And a void document is neither voidable again nor creditable.
    for action in ["void", "credit-note"] {
        let (status, _) = post(
            &h.app,
            &h.token,
            &format!("/billing/invoices/{id}/{action}"),
            json!({}),
        )
        .await;
        assert_eq!(status, StatusCode::CONFLICT, "second {action}");
    }
}

// ---- refusals ----------------------------------------------------------------

#[tokio::test]
async fn a_refused_request_writes_nothing_and_says_what_is_wrong() {
    let h = harness("bill-inv-refuse").await;
    common::seed_default_chart(&h.acc).await;
    let customer = a_customer(&h.app, &h.token).await;

    // A body that names no customer is a 422 about the field — never the 404
    // an id that does not resolve would get.
    for body in [json!({}), json!({ "customerId": "  " })] {
        let (status, answer) = post(&h.app, &h.token, "/billing/invoices", body.clone()).await;
        assert_eq!(
            status,
            StatusCode::UNPROCESSABLE_ENTITY,
            "{body} → {answer}"
        );
        assert!(
            answer["detail"]
                .as_str()
                .unwrap_or_default()
                .contains("customerId"),
            "{answer}"
        );
    }
    // A customer that does not resolve is a 404.
    let (status, _) = post(
        &h.app,
        &h.token,
        "/billing/invoices",
        json!({ "customerId": "no-such-customer" }),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    // A bad line is a 422 and leaves NO draft behind: the line set is
    // validated before the header is written.
    for lines in [
        json!([{ "description": "  " }]),
        json!([{ "description": "X", "unitPriceCents": -1 }]),
        json!([{ "description": "X", "vatRateBp": 10_001 }]),
        json!([{ "description": "X", "qtyMilli": 9_000_000_000_i64 }]),
    ] {
        let (status, answer) = post(
            &h.app,
            &h.token,
            "/billing/invoices",
            json!({ "customerId": customer, "lines": lines }),
        )
        .await;
        assert_eq!(
            status,
            StatusCode::UNPROCESSABLE_ENTITY,
            "{lines} → {answer}"
        );
    }
    let (_, body) = get(&h.app, &h.token, "/billing/invoices").await;
    assert!(
        ids(&body, "invoices").is_empty(),
        "a refusal wrote a draft: {body}"
    );

    // Money with a decimal point is a 400, refused rather than rounded.
    let (status, answer) = post(
        &h.app,
        &h.token,
        "/billing/invoices",
        json!({ "customerId": customer, "lines": [
            { "description": "Consulting", "unitPriceCents": 19.99 }] }),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{answer}");
    assert_eq!(answer["detail"], "malformed request body");

    // An empty document cannot be issued: a number of a legally unbroken
    // series is not spent on a document that says nothing.
    let empty = created_id(
        "invoice",
        post(
            &h.app,
            &h.token,
            "/billing/invoices",
            json!({ "customerId": customer }),
        )
        .await,
    );
    let (status, answer) = post(
        &h.app,
        &h.token,
        &format!("/billing/invoices/{empty}/issue"),
        json!({}),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{answer}");
    assert!(
        answer["detail"]
            .as_str()
            .unwrap_or_default()
            .contains("no lines")
    );

    // A draft cannot be voided or credited — it is deleted instead.
    for action in ["void", "credit-note"] {
        let (status, answer) = post(
            &h.app,
            &h.token,
            &format!("/billing/invoices/{empty}/{action}"),
            json!({}),
        )
        .await;
        assert_eq!(status, StatusCode::CONFLICT, "{action} → {answer}");
    }

    // A bad line in a PATCH leaves the stored lines AND the stored header as
    // they were: the set is validated before either write.
    let (status, _) = patch(
        &h.app,
        &h.token,
        &format!("/billing/invoices/{empty}"),
        json!({ "reference": "PO-1", "lines": [
            { "description": "Consulting", "unitPriceCents": 10_000, "qtyMilli": 1_000 }] }),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let (status, answer) = patch(
        &h.app,
        &h.token,
        &format!("/billing/invoices/{empty}"),
        json!({ "reference": "PO-2", "lines": [
            { "description": "Consulting", "unitPriceCents": 10_000, "qtyMilli": 1_000 },
            { "description": "" }] }),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{answer}");
    let (_, body) = get(&h.app, &h.token, &format!("/billing/invoices/{empty}")).await;
    assert_eq!(body["invoice"]["reference"], "PO-1", "the header moved");
    assert_eq!(body["invoice"]["lines"].as_array().unwrap().len(), 1);
    assert_eq!(body["invoice"]["totals"]["netCents"], 10_000);

    // An unrecognised status filter is refused rather than silently widened to
    // "everything" — a bookkeeper must never be shown drafts among issued
    // documents because of a typo.
    for bad in ["sent", "overdue", "all"] {
        let (status, answer) =
            get(&h.app, &h.token, &format!("/billing/invoices?status={bad}")).await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{bad} → {answer}");
    }
    // A blank one is simply no filter — a select on "all" sends that.
    let (status, _) = get(&h.app, &h.token, "/billing/invoices?status=").await;
    assert_eq!(status, StatusCode::OK);
}

#[tokio::test]
async fn every_route_needs_a_token_and_an_id_that_exists() {
    let h = harness("bill-inv-guards").await;
    common::seed_default_chart(&h.acc).await;
    let routes: Vec<(&str, String)> = vec![
        ("GET", "/billing/invoices".to_owned()),
        ("POST", "/billing/invoices".to_owned()),
        ("GET", "/billing/invoices/no-such-id".to_owned()),
        ("PATCH", "/billing/invoices/no-such-id".to_owned()),
        ("DELETE", "/billing/invoices/no-such-id".to_owned()),
        ("POST", "/billing/invoices/no-such-id/issue".to_owned()),
        ("POST", "/billing/invoices/no-such-id/void".to_owned()),
        (
            "POST",
            "/billing/invoices/no-such-id/credit-note".to_owned(),
        ),
    ];

    // No token: every route, including the ones that would otherwise 404 or
    // 422 — the auth guard runs before anything is looked up, so an
    // unauthenticated caller learns nothing about which ids exist.
    for (method, uri) in &routes {
        let (status, answer) = send(&h.app, with_json(method, uri, None, json!({}))).await;
        assert_eq!(
            status,
            StatusCode::UNAUTHORIZED,
            "{method} {uri} → {answer}"
        );
    }

    // With a token, an id that was never issued is a 404 on every verb that
    // names one.
    for (method, uri) in routes.iter().filter(|(_, uri)| uri.contains("no-such-id")) {
        let (status, answer) =
            send(&h.app, with_json(method, uri, Some(&h.token), json!({}))).await;
        assert_eq!(status, StatusCode::NOT_FOUND, "{method} {uri} → {answer}");
    }
}

// ---- the derived overdue flag ------------------------------------------------

#[tokio::test]
async fn only_an_issued_document_past_its_date_is_flagged_overdue() {
    let h = harness("bill-inv-overdue").await;
    common::seed_default_chart(&h.acc).await;
    let customer = a_customer(&h.app, &h.token).await;
    let id = created_id(
        "invoice",
        post(
            &h.app,
            &h.token,
            "/billing/invoices",
            json!({ "customerId": customer, "lines": three_lines() }),
        )
        .await,
    );
    let (_, body) = post(
        &h.app,
        &h.token,
        &format!("/billing/invoices/{id}/issue"),
        json!({}),
    )
    .await;
    assert_eq!(body["invoice"]["overdue"], false, "due in a fortnight");

    // The store refuses to backdate an issue (B1.08: numbers and dates ascend
    // together), so the past is planted with SQL — the flag is then tested
    // against the STORED document rather than against what today's API can
    // produce.
    let pool = PgPoolOptions::new()
        .max_connections(1)
        .connect(&database_url())
        .await
        .unwrap();
    let overdue_since = today() - Duration::days(3);
    sqlx::query("UPDATE billing_invoices SET due_date = $3 WHERE tenant_id = $1 AND id = $2")
        .bind(h.tenant.as_str())
        .bind(&id)
        .bind(overdue_since)
        .execute(&pool)
        .await
        .unwrap();

    for uri in [
        format!("/billing/invoices/{id}"),
        "/billing/invoices?status=issued".to_owned(),
    ] {
        let (_, body) = get(&h.app, &h.token, &uri).await;
        let document = body.get("invoice").unwrap_or(&body["invoices"][0]);
        assert_eq!(document["overdue"], true, "{uri} → {body}");
        assert_eq!(document["dueDate"], as_day(overdue_since));
    }

    // Voiding it stops it being owed, so it stops being overdue — without the
    // due date moving.
    let (_, body) = post(
        &h.app,
        &h.token,
        &format!("/billing/invoices/{id}/void"),
        json!({}),
    )
    .await;
    assert_eq!(body["invoice"]["status"], "void");
    assert_eq!(body["invoice"]["overdue"], false);
    assert_eq!(body["invoice"]["dueDate"], as_day(overdue_since));

    // A due date of today is not yet late: the customer has the whole day.
    let second = created_id(
        "invoice",
        post(
            &h.app,
            &h.token,
            "/billing/invoices",
            json!({ "customerId": customer, "paymentTermsDays": 0, "lines": three_lines() }),
        )
        .await,
    );
    let (_, body) = post(
        &h.app,
        &h.token,
        &format!("/billing/invoices/{second}/issue"),
        json!({}),
    )
    .await;
    assert_eq!(body["invoice"]["dueDate"], as_day(today()));
    assert_eq!(body["invoice"]["overdue"], false);
}

// ---- the wrong-tenant test (mandatory: CLAUDE.md law 1) ----------------------

#[tokio::test]
async fn another_tenants_document_is_invisible_and_untouchable_on_every_route() {
    let a = harness("bill-inv-tenant-a").await;
    common::seed_default_chart(&a.acc).await;
    let b = harness("bill-inv-tenant-b").await;
    common::seed_default_chart(&b.acc).await;

    // B's own issued document, raised through B's door.
    let b_customer = a_customer(&b.app, &b.token).await;
    let b_invoice = created_id(
        "invoice",
        post(
            &b.app,
            &b.token,
            "/billing/invoices",
            json!({ "customerId": b_customer, "reference": "B-SECRET-PO",
                    "lines": three_lines() }),
        )
        .await,
    );
    let (status, _) = post(
        &b.app,
        &b.token,
        &format!("/billing/invoices/{b_invoice}/issue"),
        json!({}),
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    // A's lists never mention it, on any filter.
    for uri in [
        "/billing/invoices",
        "/billing/invoices?status=issued",
        "/billing/invoices?status=draft",
    ] {
        let (status, body) = get(&a.app, &a.token, uri).await;
        assert_eq!(status, StatusCode::OK);
        assert!(ids(&body, "invoices").is_empty(), "{uri} leaked: {body}");
        assert!(!body.to_string().contains("B-SECRET-PO"), "{uri}: {body}");
    }

    // Every verb on B's id answers A with the same 404 an invented id gets —
    // never a 409, which would confirm the id exists AND leak its state.
    let attempts: Vec<(&str, String, Value)> = vec![
        ("GET", format!("/billing/invoices/{b_invoice}"), json!({})),
        (
            "PATCH",
            format!("/billing/invoices/{b_invoice}"),
            json!({ "reference": "TAKEN OVER" }),
        ),
        (
            "PATCH",
            format!("/billing/invoices/{b_invoice}"),
            json!({ "lines": [] }),
        ),
        (
            "DELETE",
            format!("/billing/invoices/{b_invoice}"),
            json!({}),
        ),
        (
            "POST",
            format!("/billing/invoices/{b_invoice}/issue"),
            json!({}),
        ),
        (
            "POST",
            format!("/billing/invoices/{b_invoice}/void"),
            json!({}),
        ),
        (
            "POST",
            format!("/billing/invoices/{b_invoice}/credit-note"),
            json!({}),
        ),
    ];
    for (method, uri, body) in attempts {
        let (status, answer) = send(&a.app, with_json(method, &uri, Some(&a.token), body)).await;
        assert_eq!(status, StatusCode::NOT_FOUND, "{method} {uri} → {answer}");
        assert!(
            !answer.to_string().contains("B-SECRET-PO")
                && !answer.to_string().contains("Consulting"),
            "{method} {uri} leaked what it refused: {answer}"
        );
    }

    // A cannot bill B's customer either — the link is re-checked under A's own
    // handle, so a guessed id from another tenant is a 404, not a document.
    let (status, _) = post(
        &a.app,
        &a.token,
        "/billing/invoices",
        json!({ "customerId": b_customer, "lines": three_lines() }),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    // B's document is untouched by everything A just tried, and B's series is
    // unmoved: A's failed issue must not have spent one of B's numbers.
    let (_, body) = get(&b.app, &b.token, &format!("/billing/invoices/{b_invoice}")).await;
    assert_eq!(body["invoice"]["status"], "issued");
    assert_eq!(body["invoice"]["reference"], "B-SECRET-PO");
    assert_eq!(body["invoice"]["totals"]["grossCents"], 31_664);
    assert_eq!(body["creditNotes"], json!([]));

    let b_second = created_id(
        "invoice",
        post(
            &b.app,
            &b.token,
            "/billing/invoices",
            json!({ "customerId": b_customer, "lines": three_lines() }),
        )
        .await,
    );
    let (_, body) = post(
        &b.app,
        &b.token,
        &format!("/billing/invoices/{b_second}/issue"),
        json!({}),
    )
    .await;
    assert_eq!(
        body["invoice"]["number"],
        format!("INV-{}-00002", today().year()),
        "A's refused attempts must not have consumed a number of B's series"
    );

    // Read past the routes entirely, through A's store handle, so the proof
    // does not rest on the same tenant predicate it is testing.
    let mistaken = a
        .acc
        .billing_invoice(&alo_store::BillingInvoiceId::new(b_invoice.clone()))
        .await
        .unwrap();
    assert!(mistaken.is_none());
    match a
        .acc
        .create_billing_credit_note(&alo_store::BillingInvoiceId::new(b_invoice))
        .await
    {
        Err(alo_store::StoreError::NotFound) => {}
        other => panic!("expected NotFound, got {other:?}"),
    }
}
