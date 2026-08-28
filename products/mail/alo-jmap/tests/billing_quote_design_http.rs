//! The quotation design over HTTP: saved through `PUT /billing/quotes/{id}/design`,
//! read back whole, printed into the page and set into the PDF — and, on
//! every route, invisible from another tenant.
//!
//! `alo-store`'s own suite proves the row and its tenancy; this suite is for
//! the **edge** and the **paper**: that what the studio saves is what
//! `/print` and `/pdf` carry, with the studio's numbering, colours and hidden
//! columns, and that a sent offer refuses a new design with the `409`
//! `docs/design/billing.md` publishes.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use serde_json::{Value, json};
use tower::ServiceExt;

use crate::common::{harness, send};

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

async fn put(app: &Router, token: &str, uri: &str, body: Value) -> (StatusCode, Value) {
    send(app, with_json("PUT", uri, Some(token), body)).await
}

async fn get(app: &Router, token: &str, uri: &str) -> (StatusCode, Value) {
    send(app, with_json("GET", uri, Some(token), json!({}))).await
}

/// A route's raw response — the HTML page or the PDF bytes.
async fn fetch_raw(app: &Router, token: &str, uri: &str) -> (StatusCode, String, Vec<u8>) {
    let resp = app
        .clone()
        .oneshot(with_json("GET", uri, Some(token), json!({})))
        .await
        .unwrap();
    let status = resp.status();
    let content_type = resp
        .headers()
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .unwrap_or_default()
        .to_owned();
    let bytes = axum::body::to_bytes(resp.into_body(), 64 * 1024 * 1024)
        .await
        .unwrap()
        .to_vec();
    (status, content_type, bytes)
}

fn created_id(kind: &str, (status, body): (StatusCode, Value)) -> String {
    assert_eq!(status, StatusCode::OK, "create failed: {body}");
    body[kind]["id"].as_str().unwrap().to_owned()
}

async fn a_draft_quote(app: &Router, token: &str) -> String {
    let customer = created_id(
        "customer",
        post(
            app,
            token,
            "/billing/customers",
            json!({ "name": "Acme GmbH", "addressLine1": "Hauptstraße 1", "postalCode": "10115",
                    "city": "Berlin", "country": "DE", "paymentTermsDays": 30, "currency": "EUR" }),
        )
        .await,
    );
    created_id(
        "quote",
        post(
            app,
            token,
            "/billing/quotes",
            json!({ "customerId": customer, "lines": [
                { "description": "Consulting", "unit": "hour", "qtyMilli": 2_000,
                  "unitPriceCents": 12_500, "vatRateBp": 2_100 }
            ] }),
        )
        .await,
    )
}

/// A 2×2 JPEG, encoded here so the suite carries no binary fixture.
fn tiny_jpeg_data_url() -> String {
    let rgb = image::RgbImage::from_pixel(2, 2, image::Rgb([200, 40, 40]));
    let mut bytes = Vec::new();
    image::codecs::jpeg::JpegEncoder::new_with_quality(&mut bytes, 80)
        .encode_image(&rgb)
        .unwrap();
    format!(
        "data:image/jpeg;base64,{}",
        base64::Engine::encode(&base64::engine::general_purpose::STANDARD, bytes)
    )
}

fn a_design() -> Value {
    json!({
        "theme": "modern",
        "blocks": [
            { "id": "h", "kind": "heading", "level": 1, "text": "Our <strong>proposal</strong>" },
            { "id": "p", "kind": "paragraph", "text": "<p>Delivered in three phases.</p>", "columns": 2 },
            { "id": "pricing-table", "kind": "pricing" },
            { "id": "l", "kind": "list", "ordered": true, "style": "outline", "items": "Design\n\tWireframes\nBuild" },
            { "id": "i", "kind": "image", "src": tiny_jpeg_data_url(), "caption": "Site photo" },
            { "id": "t", "kind": "table", "columns": [{ "id": "a", "label": "Milestone" }, { "id": "b", "label": "Week" }],
              "rows": [{ "id": "r", "cells": { "a": "Kick-off", "b": "1" } }] }
        ],
        "colors": { "numberMarker": "#ff0000" },
        "columns": { "unit": true, "quantity": true, "unitPrice": true, "vat": false, "net": true },
        "aFieldTheServerDoesNotKnow": { "kept": true }
    })
}

#[tokio::test]
async fn a_design_is_saved_whole_and_printed_into_the_page_and_the_pdf() {
    let h = harness("qdesign").await;
    let id = a_draft_quote(&h.app, &h.token).await;
    let route = format!("/billing/quotes/{id}/design");

    // Never designed: null, not 404.
    let (status, body) = get(&h.app, &h.token, &route).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert!(body["design"].is_null());

    // Saved whole — the field the server does not know comes back too.
    let (status, body) = put(&h.app, &h.token, &route, a_design()).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["design"], a_design());
    assert!(body["updatedAt"].is_string());
    let (_, body) = get(&h.app, &h.token, &route).await;
    assert_eq!(body["design"], a_design());

    // The page carries the content, in order, with the studio's numbering,
    // its marker colour, and without the hidden VAT column.
    let (status, content_type, bytes) =
        fetch_raw(&h.app, &h.token, &format!("/billing/quotes/{id}/print")).await;
    assert_eq!(status, StatusCode::OK);
    assert!(content_type.starts_with("text/html"), "{content_type}");
    let html = String::from_utf8(bytes).unwrap();
    let heading = html
        .find("Our <strong>proposal</strong>")
        .expect("heading printed");
    let table = html.find("<table class=\"lines\"").expect("price table");
    let list = html.find(">1.1.<").expect("outline numbering printed");
    assert!(
        heading < table && table < list,
        "blocks keep their order around the table"
    );
    assert!(html.contains("class=\"blk rich cols-2\"><p>Delivered in three phases.</p>"));
    assert!(html.contains("style=\"color:#ff0000\">1.<"));
    assert!(html.contains("<img src=\"data:image/jpeg;base64,"));
    assert!(html.contains("<th>Milestone</th><th>Week</th>"));
    assert!(html.contains("Kick-off"));
    assert!(
        !html.contains("VAT rate"),
        "the hidden column is absent from the page"
    );
    // Four headings remain — description and the three shown numeric
    // columns — and none of them is the VAT rate.
    let headings = html
        .split("<table class=\"lines\"><thead>")
        .nth(1)
        .and_then(|rest| rest.split("</thead>").next())
        .unwrap_or_default();
    assert_eq!(headings.matches("<th").count(), 4, "headings: {headings}");
    assert!(headings.contains("<th>Description</th>"));
    assert!(!headings.contains(">VAT<"), "headings: {headings}");

    // The PDF is the same document: the text is in the content stream and
    // the picture is an image object.
    let (status, content_type, bytes) =
        fetch_raw(&h.app, &h.token, &format!("/billing/quotes/{id}/pdf")).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(content_type, "application/pdf");
    assert!(bytes.starts_with(b"%PDF-1.7"));
    let text = String::from_utf8_lossy(&bytes);
    for expected in [
        "(Our proposal) Tj",
        "(Delivered in three phases.) Tj",
        "(1.1.) Tj",
        "(Wireframes) Tj",
        "(Site photo) Tj",
        "(Milestone) Tj",
        "(Kick-off) Tj",
        "/Subtype /Image",
        "/Filter /DCTDecode",
    ] {
        assert!(text.contains(expected), "PDF lacks {expected}");
    }
    assert!(
        !text.contains("(VAT RATE) Tj"),
        "the hidden column is absent from the PDF"
    );
}

#[tokio::test]
async fn the_store_rules_reach_the_wire_as_their_status_codes() {
    let h = harness("qdesign-rules").await;
    let id = a_draft_quote(&h.app, &h.token).await;
    let route = format!("/billing/quotes/{id}/design");

    let (status, _) = put(&h.app, &h.token, &route, json!([1, 2, 3])).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "not an object");

    let (status, body) = send(
        &h.app,
        Request::builder()
            .method("PUT")
            .uri(&route)
            .header("content-type", "application/json")
            .header("authorization", format!("Bearer {}", h.token))
            .body(Body::from("{not json"))
            .unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");

    // A quote the tenant never had: 404 on both verbs, and the print is
    // unaffected by the attempt.
    let (status, _) = get(&h.app, &h.token, "/billing/quotes/nope/design").await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let (status, _) = put(&h.app, &h.token, "/billing/quotes/nope/design", a_design()).await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    // Sent: frozen.
    let (status, _) = put(&h.app, &h.token, &route, a_design()).await;
    assert_eq!(status, StatusCode::OK);
    let (status, body) = post(
        &h.app,
        &h.token,
        &format!("/billing/quotes/{id}/send"),
        json!({}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let (status, body) = put(&h.app, &h.token, &route, json!({ "blocks": [] })).await;
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    assert!(
        body["detail"]
            .as_str()
            .unwrap_or_default()
            .contains("frozen")
    );
    // …but still readable and still printed.
    let (status, body) = get(&h.app, &h.token, &route).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["design"], a_design());
}

#[tokio::test]
async fn another_tenant_sees_no_design_and_cannot_write_one() {
    let alpha = harness("qdesign-alpha").await;
    let beta = harness("qdesign-beta").await;
    let id = a_draft_quote(&alpha.app, &alpha.token).await;
    let route = format!("/billing/quotes/{id}/design");
    let (status, _) = put(&alpha.app, &alpha.token, &route, a_design()).await;
    assert_eq!(status, StatusCode::OK);

    // Beta's harness shares the database; the id is real, and answers as one
    // that never existed.
    let (status, body) = get(&beta.app, &beta.token, &route).await;
    assert_eq!(status, StatusCode::NOT_FOUND, "{body}");
    let (status, _) = put(&beta.app, &beta.token, &route, json!({ "blocks": [] })).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let (status, _, _) =
        fetch_raw(&beta.app, &beta.token, &format!("/billing/quotes/{id}/pdf")).await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    let (_, body) = get(&alpha.app, &alpha.token, &route).await;
    assert_eq!(body["design"], a_design(), "alpha's design is untouched");
}
