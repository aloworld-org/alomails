//! The lead import over HTTP (B2.09), driven through the real router over a
//! real Postgres.
//!
//! `alo-store`'s own suite proves the records, the duplicate rules and the
//! tenant wall. What matters here is the **edge**: the auth guard on both
//! routes, the mapping arriving as a real (percent-encoded) query string, the
//! `422` that carries the per-row report rather than a sentence about a file,
//! and that a neighbour's board answers exactly as a board that never existed.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use serde_json::Value;

use crate::common::{Harness, harness, send};

/// The same fixture the store suite imports — a semicolon file from a European
/// spreadsheet, blank line and repeated company included.
const LEADS: &str = include_str!("../../../../platform/alo-store/tests/fixtures/crm_leads.csv");

/// Uploads a file to an import route with a bearer token.
async fn upload(app: &Router, token: &str, uri: &str, file: &str) -> (StatusCode, Value) {
    let req = Request::builder()
        .method("POST")
        .uri(uri)
        .header("content-type", "text/csv")
        .header("authorization", format!("Bearer {token}"))
        .body(Body::from(file.to_owned()))
        .unwrap();
    send(app, req).await
}

/// The same upload with no bearer at all.
async fn anonymous(app: &Router, uri: &str) -> StatusCode {
    let req = Request::builder()
        .method("POST")
        .uri(uri)
        .header("content-type", "text/csv")
        .body(Body::from("Company\nAcme\n"))
        .unwrap();
    send(app, req).await.0
}

async fn get(app: &Router, token: &str, uri: &str) -> (StatusCode, Value) {
    let req = Request::builder()
        .uri(uri)
        .header("authorization", format!("Bearer {token}"))
        .body(Body::empty())
        .unwrap();
    send(app, req).await
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

#[tokio::test]
async fn both_import_routes_are_shut_without_a_token() {
    let h = harness("crmimp-auth").await;
    for uri in [
        "/crm/imports/leads/preview?pipelineId=pip_1",
        "/crm/imports/leads?pipelineId=pip_1",
    ] {
        assert_eq!(
            anonymous(&h.app, uri).await,
            StatusCode::UNAUTHORIZED,
            "{uri}"
        );
    }
}

#[tokio::test]
async fn a_request_without_a_board_is_refused_before_the_file_is_read() {
    let h = harness("crmimp-noboard").await;
    for uri in ["/crm/imports/leads/preview", "/crm/imports/leads"] {
        let (status, body) = upload(&h.app, &h.token, uri, LEADS).await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{uri}: {body}");
        assert!(
            body["detail"].as_str().unwrap().contains("pipelineId"),
            "{body}"
        );
    }
}

#[tokio::test]
async fn the_preview_reports_the_file_and_the_commit_lands_it() {
    let h = harness("crmimp-arc").await;
    let (pipeline, stages) = board(&h).await;

    // ---- preview: nothing written, the mapping guessed and reported --------
    let (status, body) = upload(
        &h.app,
        &h.token,
        &format!("/crm/imports/leads/preview?pipelineId={pipeline}"),
        LEADS,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let report = &body["import"];
    assert_eq!(report["committed"], false);
    assert_eq!(report["encoding"], "utf-8");
    assert_eq!(report["delimiter"], ";");
    assert_eq!(report["totalRows"], 6);
    assert_eq!(report["counts"]["create"], 4);
    assert_eq!(report["counts"]["duplicates"], 2);
    assert_eq!(report["counts"]["errors"], 0);
    assert_eq!(report["mapping"]["company"], "Company");
    assert_eq!(report["mapping"]["email"], "E-mail");
    assert_eq!(report["mapping"]["title"], Value::Null);
    assert_eq!(report["columns"][0], "Company");
    // The server's reading of the row, not the row: integer cents and an ISO
    // day, which is what will be stored.
    assert_eq!(report["leads"][0]["line"], 2);
    assert_eq!(report["leads"][0]["valueCents"], 1_250_000);
    assert_eq!(report["leads"][0]["expectedClose"], "2026-09-30");
    assert_eq!(
        report["leads"][0]["id"],
        Value::Null,
        "a preview writes no id"
    );
    assert_eq!(report["duplicates"][1]["reason"], "domain");
    assert_eq!(report["duplicates"][1]["matched"], "acme.example");

    let (status, body) = get(&h.app, &h.token, "/crm/deals").await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(
        body["deals"].as_array().unwrap().len(),
        0,
        "nothing written"
    );

    // ---- commit ------------------------------------------------------------
    let (status, body) = upload(
        &h.app,
        &h.token,
        &format!(
            "/crm/imports/leads?pipelineId={pipeline}&stageId={}",
            stages[1]
        ),
        LEADS,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let report = &body["import"];
    assert_eq!(report["committed"], true);
    assert_eq!(report["counts"]["create"], 4);
    assert!(report["leads"][0]["id"].is_string(), "{report}");

    let (status, body) = get(&h.app, &h.token, "/crm/deals").await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let deals = body["deals"].as_array().unwrap();
    assert_eq!(deals.len(), 4);
    assert!(deals.iter().all(|d| d["stageId"] == stages[1]));
    assert!(deals.iter().all(|d| d["state"] == "open"));
    let acme = deals
        .iter()
        .find(|d| d["title"] == "Acme GmbH")
        .expect("the first lead");
    assert_eq!(acme["contactEmail"], "ada@acme.example");
    assert_eq!(acme["valueCents"], 1_250_000);
    assert_eq!(acme["currency"], "EUR");
}

#[tokio::test]
async fn a_mapping_is_a_query_string_and_percent_encoding_survives_it() {
    let h = harness("crmimp-mapping").await;
    let (pipeline, _stages) = board(&h).await;

    // A German export whose headers this product does not guess.
    let file = "Firma;Ansprechpartner;Kontakt;Umsatz\n\
                Acme GmbH;Ada;ada@acme.example;12.500,00\n";
    // Unmapped: the guess finds nothing it recognises, and a file nothing can
    // be read from is one refusal naming what is missing — never a page of
    // blank leads, and never the same row error two thousand times.
    let (status, body) = upload(
        &h.app,
        &h.token,
        &format!("/crm/imports/leads/preview?pipelineId={pipeline}"),
        file,
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
    assert!(
        body["detail"]
            .as_str()
            .unwrap()
            .contains("no column is mapped to a title or a company name"),
        "{body}"
    );

    // Mapped by hand, with a space and an accent in the URL.
    let uri = format!(
        "/crm/imports/leads/preview?pipelineId={pipeline}&company=Firma\
         &contactName=Ansprechpartner&email=Kontakt&value=Umsatz"
    );
    let (status, body) = upload(&h.app, &h.token, &uri, file).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let lead = &body["import"]["leads"][0];
    assert_eq!(lead["companyName"], "Acme GmbH");
    assert_eq!(lead["contactName"], "Ada");
    assert_eq!(lead["contactEmail"], "ada@acme.example");
    assert_eq!(
        lead["valueCents"], 1_250_000,
        "a German amount, read exactly"
    );

    // A percent-encoded column name reaches the store as it was written.
    let accented = "Société;Montant HT\nAcme SA;900\n";
    let uri = format!(
        "/crm/imports/leads/preview?pipelineId={pipeline}\
         &company=Soci%C3%A9t%C3%A9&value=Montant%20HT"
    );
    let (status, body) = upload(&h.app, &h.token, &uri, accented).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["import"]["mapping"]["company"], "Société");
    assert_eq!(body["import"]["leads"][0]["companyName"], "Acme SA");
    assert_eq!(body["import"]["leads"][0]["valueCents"], 90_000);

    // A column the file does not have is a refusal, never a blank import.
    let uri = format!("/crm/imports/leads/preview?pipelineId={pipeline}&value=Turnover");
    let (status, body) = upload(&h.app, &h.token, &uri, file).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
    assert!(
        body["detail"]
            .as_str()
            .unwrap()
            .contains("no column mapped"),
        "{body}"
    );
}

#[tokio::test]
async fn a_commit_with_a_bad_row_is_a_422_carrying_the_report() {
    let h = harness("crmimp-refusal").await;
    let (pipeline, _stages) = board(&h).await;

    let file = "Company,Email,Amount\n\
                Acme GmbH,ada@acme.example,100\n\
                Beta BV,not-an-address,200\n";
    let (status, body) = upload(
        &h.app,
        &h.token,
        &format!("/crm/imports/leads?pipelineId={pipeline}"),
        file,
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
    // A problem, and the report inside it — the refusal a person can act on.
    assert_eq!(body["status"], 422);
    assert!(
        body["detail"]
            .as_str()
            .unwrap()
            .contains("nothing was written")
    );
    let report = &body["import"];
    assert_eq!(report["committed"], false);
    assert_eq!(report["counts"]["errors"], 1);
    assert_eq!(report["errors"][0]["line"], 3);
    assert!(
        !report["errors"][0]["rule"]
            .as_str()
            .unwrap()
            .contains("not-an-address"),
        "a refusal never quotes the row"
    );

    let (status, body) = get(&h.app, &h.token, "/crm/deals").await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(
        body["deals"].as_array().unwrap().len(),
        0,
        "all-or-nothing: not even the good row"
    );
}

#[tokio::test]
async fn a_file_that_is_not_a_lead_list_is_refused_as_a_422() {
    let h = harness("crmimp-unreadable").await;
    let (pipeline, _stages) = board(&h).await;
    let uri = format!("/crm/imports/leads/preview?pipelineId={pipeline}");

    for (file, rule) in [
        ("", "empty"),
        ("Company,Email\nAcme,a@b.example,extra\n", "more fields"),
    ] {
        let (status, body) = upload(&h.app, &h.token, &uri, file).await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{file:?}: {body}");
        assert!(
            body["detail"].as_str().unwrap().contains(rule),
            "expected {rule:?} in {body}"
        );
    }
}

#[tokio::test]
async fn a_neighbours_board_answers_as_a_board_that_never_existed() {
    let a = harness("crmimp-wall-a").await;
    let b = harness("crmimp-wall-b").await;
    let (a_pipeline, a_stages) = board(&a).await;

    for uri in [
        format!("/crm/imports/leads/preview?pipelineId={a_pipeline}"),
        format!("/crm/imports/leads?pipelineId={a_pipeline}"),
        format!(
            "/crm/imports/leads?pipelineId={a_pipeline}&stageId={}",
            a_stages[0]
        ),
        "/crm/imports/leads?pipelineId=pip_nope".to_owned(),
    ] {
        let (status, body) = upload(&b.app, &b.token, &uri, LEADS).await;
        assert_eq!(status, StatusCode::NOT_FOUND, "{uri}: {body}");
    }
    let (status, body) = get(&a.app, &a.token, "/crm/deals").await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["deals"].as_array().unwrap().len(), 0, "A is untouched");
}
