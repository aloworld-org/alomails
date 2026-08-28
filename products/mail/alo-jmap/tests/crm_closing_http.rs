//! The closing flow over HTTP (B2.08), driven through the real router over a
//! real Postgres: the won-deal handoff to billing, and the pipeline report in
//! both of its representations.
//!
//! `alo-store`'s own suites prove the records, the arithmetic and the tenant
//! wall. What matters here is the **edge**: the auth guard on all four routes,
//! the status codes `docs/design/crm.md` publishes, that the handoff answers
//! the created document **and** the deal it changed, that the CSV is served as
//! a file with the headers an export carries, and that a neighbour's board and
//! deal answer exactly as ids that never existed.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use serde_json::{Value, json};

use crate::common::{Harness, get_text, harness, send};

// ---- request helpers ---------------------------------------------------------

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

async fn get(app: &Router, token: &str, uri: &str) -> (StatusCode, Value) {
    let req = Request::builder()
        .uri(uri)
        .header("authorization", format!("Bearer {token}"))
        .body(Body::empty())
        .unwrap();
    send(app, req).await
}

/// An unauthenticated request — no bearer at all.
async fn anonymous(app: &Router, method: &str, uri: &str) -> StatusCode {
    let req = Request::builder()
        .method(method)
        .uri(uri)
        .header("content-type", "application/json")
        .body(Body::from("{}"))
        .unwrap();
    send(app, req).await.0
}

/// The tenant's seeded board, as (pipeline id, stage ids left to right).
async fn board(h: &Harness) -> (String, Vec<String>) {
    let (status, body) = get(&h.app, &h.token, "/crm/pipelines").await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let pipeline = body["pipelines"][0]["id"].as_str().unwrap().to_owned();
    let (status, body) = get(
        &h.app,
        &h.token,
        &format!("/crm/pipelines/{pipeline}/stages"),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let stages = body["stages"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| s["id"].as_str().unwrap().to_owned())
        .collect();
    (pipeline, stages)
}

/// A priced lead on the seeded board, moved into the winning column.
async fn won_lead(h: &Harness, title: &str, value_cents: i64) -> String {
    let (pipeline, stages) = board(h).await;
    let (status, body) = post(
        &h.app,
        &h.token,
        "/crm/deals",
        json!({
            "pipelineId": pipeline,
            "stageId": stages[0],
            "title": title,
            "companyName": "Acme GmbH",
            "contactName": "Ada",
            "contactEmail": "ada@acme.example",
            "valueCents": value_cents,
        }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let deal = body["deal"]["id"].as_str().unwrap().to_owned();
    // The seeded board's fourth column is the winning one.
    let (status, body) = post(
        &h.app,
        &h.token,
        &format!("/crm/deals/{deal}/stage"),
        json!({ "stageId": stages[3] }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["deal"]["state"], "won");
    deal
}

// ---- the handoff -------------------------------------------------------------

#[tokio::test]
async fn a_won_deal_raises_a_draft_invoice_and_the_customer_it_needs() {
    let h = harness("crmcls-invoice").await;
    let deal = won_lead(&h, "Renewal — Acme GmbH", 250_000).await;

    let (status, body) = post(
        &h.app,
        &h.token,
        &format!("/crm/deals/{deal}/invoice"),
        json!({ "vatRateBp": 1900, "country": "de" }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    // The document: a draft, no number, the deal's currency, one line, and
    // totals the server computed.
    assert_eq!(body["invoice"]["status"], "draft");
    assert!(body["invoice"]["number"].is_null());
    assert_eq!(body["invoice"]["currency"], "EUR");
    assert_eq!(body["invoice"]["lines"].as_array().map(Vec::len), Some(1));
    assert_eq!(
        body["invoice"]["lines"][0]["description"],
        "Renewal — Acme GmbH"
    );
    assert_eq!(body["invoice"]["lines"][0]["unitPriceCents"], 250_000);
    assert_eq!(body["invoice"]["lines"][0]["qtyMilli"], 1_000);
    assert_eq!(body["invoice"]["totals"]["netCents"], 250_000);
    assert_eq!(body["invoice"]["totals"]["vatCents"], 47_500);
    assert_eq!(body["invoice"]["totals"]["grossCents"], 297_500);
    // And the deal, because raising the document changed it.
    let customer = body["deal"]["customerId"].as_str().unwrap().to_owned();
    assert_eq!(body["invoice"]["customerId"], customer.as_str());
    assert_eq!(body["deal"]["state"], "won");

    // The customer is a real one of the tenant, created from the lead.
    let (status, listed) = get(&h.app, &h.token, "/billing/customers").await;
    assert_eq!(status, StatusCode::OK, "{listed}");
    assert_eq!(listed["customers"].as_array().map(Vec::len), Some(1));
    assert_eq!(listed["customers"][0]["id"], customer.as_str());
    assert_eq!(listed["customers"][0]["name"], "Acme GmbH");
    assert_eq!(listed["customers"][0]["country"], "DE");
    assert_eq!(listed["customers"][0]["email"], "ada@acme.example");

    // A second document bills the same company rather than a twin of it.
    let (status, body) = post(
        &h.app,
        &h.token,
        &format!("/crm/deals/{deal}/quote"),
        json!({ "vatRateBp": 1900 }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["quote"]["status"], "draft");
    assert_eq!(body["quote"]["customerId"], customer.as_str());
    assert_eq!(body["quote"]["totals"]["grossCents"], 297_500);
    let (_, listed) = get(&h.app, &h.token, "/billing/customers").await;
    assert_eq!(listed["customers"].as_array().map(Vec::len), Some(1));
}

#[tokio::test]
async fn what_the_handoff_refuses_at_the_edge() {
    let h = harness("crmcls-refuse").await;
    let (_, stages) = board(&h).await;
    let deal = won_lead(&h, "Renewal — Acme GmbH", 250_000).await;

    // A priced deal with no rate: the rule, not a rounded guess.
    let (status, body) = post(
        &h.app,
        &h.token,
        &format!("/crm/deals/{deal}/invoice"),
        json!({ "country": "DE" }),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
    assert!(
        body["detail"].as_str().unwrap().contains("VAT rate"),
        "{body}"
    );

    // A rate that is not an integer number of basis points is a `400` at the
    // parser, never a rounded document.
    let (status, _) = post(
        &h.app,
        &h.token,
        &format!("/crm/deals/{deal}/invoice"),
        json!({ "vatRateBp": 19.5, "country": "DE" }),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);

    // A lead with no country.
    let (status, body) = post(
        &h.app,
        &h.token,
        &format!("/crm/deals/{deal}/quote"),
        json!({ "vatRateBp": 2100 }),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
    assert!(
        body["detail"].as_str().unwrap().contains("two-letter"),
        "{body}"
    );

    // A lost deal raises nothing.
    let (status, body) = post(
        &h.app,
        &h.token,
        &format!("/crm/deals/{deal}/stage"),
        json!({ "stageId": stages[4], "lostReason": "Price" }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let (status, body) = post(
        &h.app,
        &h.token,
        &format!("/crm/deals/{deal}/invoice"),
        json!({ "vatRateBp": 2100, "country": "DE" }),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
    assert!(
        body["detail"].as_str().unwrap().contains("was lost"),
        "{body}"
    );

    // Nothing at all was written along the way.
    let (_, listed) = get(&h.app, &h.token, "/billing/customers").await;
    assert_eq!(listed["customers"].as_array().map(Vec::len), Some(0));
    let (_, drafts) = get(&h.app, &h.token, "/billing/invoices").await;
    assert_eq!(drafts["invoices"].as_array().map(Vec::len), Some(0));
}

#[tokio::test]
async fn a_deal_worth_nothing_raises_an_empty_draft_from_an_empty_body() {
    let h = harness("crmcls-empty").await;
    let (pipeline, stages) = board(&h).await;
    let (status, body) = post(
        &h.app,
        &h.token,
        "/crm/deals",
        json!({
            "pipelineId": pipeline,
            "stageId": stages[0],
            "title": "Scoping — Acme GmbH",
            "companyName": "Acme GmbH",
        }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let deal = body["deal"]["id"].as_str().unwrap().to_owned();

    // No rate is needed — there is no line to rate — but a country still is,
    // because there is a customer to create.
    let (status, body) = post(
        &h.app,
        &h.token,
        &format!("/crm/deals/{deal}/quote"),
        json!({ "country": "NL" }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["quote"]["lines"].as_array().map(Vec::len), Some(0));
    assert_eq!(body["quote"]["totals"]["grossCents"], 0);
    assert!(body["deal"]["customerId"].as_str().is_some());
}

// ---- the report --------------------------------------------------------------

#[tokio::test]
async fn a_pipeline_report_answers_the_open_board_and_the_periods_outcomes() {
    let h = harness("crmcls-report").await;
    let (pipeline, stages) = board(&h).await;
    let year = time::OffsetDateTime::now_utc().year();
    let period = format!("from={year}-01-01&to={year}-12-31");

    // Two open deals and one won.
    for (title, value) in [("Pilot — Beta", 50_000), ("Expansion", 1_000_000)] {
        let (status, body) = post(
            &h.app,
            &h.token,
            "/crm/deals",
            json!({
                "pipelineId": pipeline,
                "stageId": stages[0],
                "title": title,
                "valueCents": value,
            }),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{body}");
    }
    won_lead(&h, "Renewal — Acme GmbH", 250_000).await;

    let (status, body) = get(
        &h.app,
        &h.token,
        &format!("/crm/reports/pipeline?pipelineId={pipeline}&{period}"),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let report = &body["report"];
    assert_eq!(report["pipelineId"], pipeline.as_str());
    assert_eq!(report["pipelineName"], "Sales");
    assert_eq!(report["from"], format!("{year}-01-01"));
    assert!(report["openAsOf"].as_str().unwrap().ends_with('Z'));
    let eur = &report["currencies"][0];
    assert_eq!(eur["currency"], "EUR");
    assert_eq!(eur["stages"].as_array().map(Vec::len), Some(5));
    assert_eq!(eur["stages"][0]["name"], "New");
    assert_eq!(eur["stages"][0]["open"]["dealCount"], 2);
    assert_eq!(eur["stages"][0]["open"]["valueCents"], 1_050_000);
    assert_eq!(eur["stages"][3]["isWon"], true);
    assert_eq!(eur["open"]["valueCents"], 1_050_000);
    assert_eq!(eur["won"]["dealCount"], 1);
    assert_eq!(eur["won"]["valueCents"], 250_000);
    assert_eq!(eur["lost"]["dealCount"], 0);
    assert_eq!(eur["winRateBp"], 10_000);

    // A period with nothing in it keeps the open board and empties the
    // outcomes — the two halves really are answered differently.
    let (status, body) = get(
        &h.app,
        &h.token,
        &format!("/crm/reports/pipeline?pipelineId={pipeline}&from=2019-01-01&to=2019-12-31"),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let eur = &body["report"]["currencies"][0];
    assert_eq!(eur["open"]["valueCents"], 1_050_000);
    assert_eq!(eur["won"]["dealCount"], 0);
    assert!(eur["winRateBp"].is_null(), "a rate over nothing is unasked");

    // ---- the same figures as a file --------------------------------------
    let (status, csv) = get_text(
        &h.app,
        &h.token,
        &format!("/crm/reports/pipeline.csv?pipelineId={pipeline}&{period}"),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{csv}");
    let lines: Vec<&str> = csv.split("\r\n").filter(|l| !l.is_empty()).collect();
    assert!(lines[0].starts_with("row,pipeline,periodFrom,periodTo,currency"));
    assert!(
        lines
            .iter()
            .any(|l| l.starts_with("stage,Sales,") && l.ends_with(",New,2,10500.00,")),
        "{csv}"
    );
    assert!(
        lines
            .iter()
            .any(|l| l.starts_with("won,Sales,") && l.ends_with(",,1,2500.00,100.00")),
        "{csv}"
    );
    assert!(
        lines
            .iter()
            .any(|l| l.contains(",EUR,,0,0.00,") && l.starts_with("lost,")),
        "{csv}"
    );

    // ---- and it is served as a file, not a page --------------------------
    let req = Request::builder()
        .uri(format!(
            "/crm/reports/pipeline.csv?pipelineId={pipeline}&{period}"
        ))
        .header("authorization", format!("Bearer {}", h.token))
        .body(Body::empty())
        .unwrap();
    let resp = tower::ServiceExt::oneshot(h.app.clone(), req)
        .await
        .unwrap();
    let headers = resp.headers();
    assert_eq!(headers["content-type"], "text/csv; charset=utf-8");
    assert!(
        headers["content-disposition"]
            .to_str()
            .unwrap()
            .starts_with(&format!("attachment; filename=\"pipeline-{pipeline}-")),
        "{headers:?}"
    );
    assert_eq!(headers["x-content-type-options"], "nosniff");
    assert_eq!(headers["cache-control"], "no-store");
}

#[tokio::test]
async fn a_report_states_its_board_and_its_period_or_is_refused() {
    let h = harness("crmcls-report-422").await;
    let (pipeline, _) = board(&h).await;

    for (uri, names) in [
        (
            "/crm/reports/pipeline?from=2026-01-01&to=2026-12-31",
            "pipelineId",
        ),
        ("/crm/reports/pipeline?pipelineId=x&to=2026-12-31", "from"),
        ("/crm/reports/pipeline?pipelineId=x&from=2026-01-01", "to"),
        (
            "/crm/reports/pipeline?pipelineId=x&from=01/01/2026&to=2026-12-31",
            "YYYY-MM-DD",
        ),
    ] {
        let (status, body) = get(&h.app, &h.token, uri).await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{uri}: {body}");
        assert!(
            body["detail"].as_str().unwrap().contains(names),
            "{uri}: {body}"
        );
    }

    // A period that ends before it starts is the store's rule, at the edge.
    let (status, body) = get(
        &h.app,
        &h.token,
        &format!("/crm/reports/pipeline?pipelineId={pipeline}&from=2026-03-03&to=2026-03-02"),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
    assert!(
        body["detail"].as_str().unwrap().contains("ends before"),
        "{body}"
    );

    // A board that is not this tenant's is the same `404` an invented id gets.
    let (status, body) = get(
        &h.app,
        &h.token,
        "/crm/reports/pipeline?pipelineId=pip_nope&from=2026-01-01&to=2026-12-31",
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND, "{body}");
}

// ---- the guards --------------------------------------------------------------

#[tokio::test]
async fn every_closing_route_needs_a_token() {
    let h = harness("crmcls-401").await;
    for (method, uri) in [
        ("POST", "/crm/deals/dea_1/quote"),
        ("POST", "/crm/deals/dea_1/invoice"),
        (
            "GET",
            "/crm/reports/pipeline?pipelineId=p&from=2026-01-01&to=2026-12-31",
        ),
        (
            "GET",
            "/crm/reports/pipeline.csv?pipelineId=p&from=2026-01-01&to=2026-12-31",
        ),
    ] {
        assert_eq!(
            anonymous(&h.app, method, uri).await,
            StatusCode::UNAUTHORIZED,
            "{method} {uri}"
        );
    }
}

#[tokio::test]
async fn a_neighbours_deal_and_board_answer_as_ids_that_never_existed() {
    let a = harness("crmcls-a").await;
    let b = harness("crmcls-b").await;
    let deal = won_lead(&a, "Renewal — Acme GmbH", 250_000).await;
    let (a_board, _) = board(&a).await;

    for uri in [
        format!("/crm/deals/{deal}/quote"),
        format!("/crm/deals/{deal}/invoice"),
    ] {
        let (status, body) = post(
            &b.app,
            &b.token,
            &uri,
            json!({ "vatRateBp": 2100, "country": "DE" }),
        )
        .await;
        assert_eq!(status, StatusCode::NOT_FOUND, "{uri}: {body}");
    }
    let (status, body) = get(
        &b.app,
        &b.token,
        &format!("/crm/reports/pipeline?pipelineId={a_board}&from=2026-01-01&to=2026-12-31"),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND, "{body}");

    // Nothing of the neighbour's leaked, and nothing of ours was written.
    let (_, listed) = get(&b.app, &b.token, "/billing/customers").await;
    assert_eq!(listed["customers"].as_array().map(Vec::len), Some(0));
    let (_, mine) = get(&a.app, &a.token, "/crm/deals").await;
    assert!(mine["deals"][0]["customerId"].is_null(), "{mine}");
}
