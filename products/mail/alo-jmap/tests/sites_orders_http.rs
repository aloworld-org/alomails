//! The `/sites/{id}/orders*` inbox (ADR 0036 / ADR 0041, S2.12b–c2), driven
//! through the real router over a real Postgres, with the orders themselves
//! written by the anonymous public door exactly as a visitor writes them.
//!
//! `alo-store`'s suite proves the storage and the tenant boundary around an
//! order. What this suite pins is the **edge the order screen is built on**:
//! the shape the inbox answers (including the currency exponent a screen must
//! not derive itself), the four status words, the spreadsheet-safe export, the
//! auth guard, and — mandatory — that another tenant's orders are invisible
//! and untouchable on every verb.

#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use std::sync::Arc;

use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use serde_json::{Value, json};
use sqlx::postgres::PgPoolOptions;

use alo_store::{
    AccountStore, BlobStore, OrderContact, OrderRequestLine, SiteCatalogAvailability,
    SiteCatalogId, SiteCatalogInput, SiteCatalogItemInput, SiteId, SiteOrderId, SitePublicStore,
    normalize_order_contact,
};

use common::{Harness, harness_on, harness_with_blobs, send};

// ---- driving the router ------------------------------------------------------

async fn get(app: &Router, token: &str, uri: &str) -> (StatusCode, Value) {
    let req = Request::builder()
        .method("GET")
        .uri(uri)
        .header("authorization", format!("Bearer {token}"))
        .body(Body::empty())
        .unwrap();
    send(app, req).await
}

async fn put(app: &Router, token: &str, uri: &str, body: Value) -> (StatusCode, Value) {
    let req = Request::builder()
        .method("PUT")
        .uri(uri)
        .header("content-type", "application/json")
        .header("authorization", format!("Bearer {token}"))
        .body(Body::from(body.to_string()))
        .unwrap();
    send(app, req).await
}

async fn delete(app: &Router, token: &str, uri: &str) -> (StatusCode, Value) {
    let req = Request::builder()
        .method("DELETE")
        .uri(uri)
        .header("authorization", format!("Bearer {token}"))
        .body(Body::empty())
        .unwrap();
    send(app, req).await
}

/// GETs the export, which answers a CSV document rather than JSON.
async fn get_csv(
    app: &Router,
    token: &str,
    uri: &str,
) -> (StatusCode, axum::http::HeaderMap, String) {
    use tower::ServiceExt;
    let req = Request::builder()
        .method("GET")
        .uri(uri)
        .header("authorization", format!("Bearer {token}"))
        .body(Body::empty())
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    let status = resp.status();
    let headers = resp.headers().clone();
    let bytes = axum::body::to_bytes(resp.into_body(), 8 * 1024 * 1024)
        .await
        .unwrap();
    (status, headers, String::from_utf8(bytes.to_vec()).unwrap())
}

// ---- the fixture -------------------------------------------------------------

/// A subdomain unique to this harness run — the namespace is global.
fn sub(tag: &str, h: &Harness) -> String {
    let salt: String = h
        .tenant
        .as_str()
        .chars()
        .filter(char::is_ascii_alphanumeric)
        .map(|c| c.to_ascii_lowercase())
        .take(16)
        .collect();
    format!("{tag}{salt}")
}

fn item<'a>(name: &'a str, slug: &'a str, price: Option<i64>) -> SiteCatalogItemInput<'a> {
    SiteCatalogItemInput {
        category: None,
        name,
        slug,
        description: None,
        price_cents: price,
        price_note: None,
        image: None,
        availability: SiteCatalogAvailability::Available,
        position: 0,
    }
}

/// A visitor whose note starts with a character a spreadsheet would evaluate.
fn contact(name: &str) -> OrderContact {
    normalize_order_contact(
        name,
        "ada@example.test",
        "+32 2 555 01",
        "=leave at the door",
    )
    .unwrap()
}

fn asked(slug: &str, quantity: i32) -> OrderRequestLine {
    OrderRequestLine {
        item_slug: slug.to_owned(),
        quantity,
    }
}

async fn public_store(blobs: BlobStore) -> SitePublicStore {
    let pool = PgPoolOptions::new()
        .max_connections(4)
        .connect(&common::database_url())
        .await
        .unwrap();
    SitePublicStore::new(pool, blobs)
}

/// A live site with one published, orderable catalog: a priced loaf and a cake
/// quoted by hand. Built through the store because what is under test is the
/// order edge, not the catalog edge — that has a suite of its own.
async fn orderable_site(
    account: &AccountStore,
    subdomain: &str,
) -> (SiteId, SiteCatalogId, String) {
    let site = account.create_site("Bakery", subdomain).await.unwrap();
    let home = account
        .create_site_page(&site, "Home", "", true)
        .await
        .unwrap();
    let catalog = account
        .create_site_catalog(
            &site,
            &SiteCatalogInput {
                name: "Saturday bake",
                currency: "EUR",
                orders_enabled: true,
            },
        )
        .await
        .unwrap();
    for input in [
        item("Sourdough", "sourdough", Some(450)),
        item("Wedding cake", "wedding-cake", None),
    ] {
        account
            .create_site_catalog_item(&site, &catalog, &input)
            .await
            .unwrap();
    }
    account
        .set_page_sections(
            &site,
            &home,
            json!({
                "schema_version": 1,
                "sections": [{
                    "type": "catalog",
                    "catalog_id": catalog.as_str(),
                    "heading": "Order for Saturday"
                }]
            }),
        )
        .await
        .unwrap();
    account.publish_site(&site).await.unwrap();
    (site, catalog, subdomain.to_owned())
}

async fn place(
    public: &SitePublicStore,
    catalog: &SiteCatalogId,
    who: &str,
    lines: &[OrderRequestLine],
) -> SiteOrderId {
    public
        .place_public_order(catalog.as_str(), &contact(who), lines)
        .await
        .unwrap()
        .expect("a live orderable catalog accepts the order")
}

// ---- the inbox ---------------------------------------------------------------

/// The whole owner arc over the wire: what the visitor asked for arrives with
/// its lines and its money, the status moves both ways, a word that is not a
/// status is refused in the store's own sentence, and deleting an order that
/// carries somebody's name and phone number really deletes it.
#[tokio::test]
async fn the_inbox_reads_an_order_with_its_lines_and_moves_it_through_the_workflow() {
    let (h, blobs) = harness_with_blobs("sites-orders-inbox").await;
    let (site, catalog, _) = orderable_site(&h.acc, &sub("orders", &h)).await;
    let public = public_store(blobs).await;

    let first = place(
        &public,
        &catalog,
        "Ada Lovelace",
        &[asked("sourdough", 3), asked("wedding-cake", 1)],
    )
    .await;
    let second = place(&public, &catalog, "Grace Hopper", &[asked("sourdough", 1)]).await;

    let (status, body) = get(&h.app, &h.token, &format!("/sites/{site}/orders")).await;
    assert_eq!(status, StatusCode::OK, "inbox failed: {body}");
    let orders = body["orders"].as_array().unwrap();
    assert_eq!(orders.len(), 2, "{body}");
    // Newest first — the screen reads top-down and never re-sorts.
    assert_eq!(orders[0]["id"], json!(second.as_str()));
    let order = &orders[1];
    assert_eq!(order["id"], json!(first.as_str()));
    assert_eq!(order["customerName"], json!("Ada Lovelace"));
    assert_eq!(order["customerEmail"], json!("ada@example.test"));
    assert_eq!(order["customerPhone"], json!("+32 2 555 01"));
    assert_eq!(order["note"], json!("=leave at the door"));
    assert_eq!(order["catalogName"], json!("Saturday bake"));
    assert_eq!(order["currency"], json!("EUR"));
    // The exponent travels with the currency: the screen divides by nothing it
    // guessed, and a yen order would carry 0 here.
    assert_eq!(order["currencyExponent"], json!(2));
    assert_eq!(
        order["totalCents"],
        json!(1_350),
        "3 x 450; the unpriced line adds nothing"
    );
    assert_eq!(order["status"], json!("new"));
    assert!(
        order["receivedAt"]
            .as_str()
            .is_some_and(|at| at.contains('T'))
    );
    let lines = order["lines"].as_array().unwrap();
    assert_eq!(lines.len(), 2);
    assert_eq!(lines[0]["itemSlug"], json!("sourdough"));
    assert_eq!(lines[0]["itemName"], json!("Sourdough"));
    assert_eq!(lines[0]["quantity"], json!(3));
    assert_eq!(lines[0]["unitPriceCents"], json!(450));
    assert_eq!(lines[0]["lineTotalCents"], json!(1_350));
    // Quoted by hand: no price at all is not a price of zero.
    assert_eq!(lines[1]["itemSlug"], json!("wedding-cake"));
    assert_eq!(lines[1]["unitPriceCents"], json!(null));
    assert_eq!(lines[1]["lineTotalCents"], json!(null));

    // The workflow moves in both directions: an order cancelled by mistake is
    // confirmed again rather than re-typed.
    let uri = format!("/sites/{site}/orders/{first}");
    let (status, moved) = put(&h.app, &h.token, &uri, json!({ "status": "confirmed" })).await;
    assert_eq!(status, StatusCode::OK, "confirm failed: {moved}");
    assert_eq!(moved["status"], json!("confirmed"));
    assert_eq!(moved["lines"].as_array().unwrap().len(), 2, "{moved}");
    let (status, moved) = put(&h.app, &h.token, &uri, json!({ "status": "cancelled" })).await;
    assert_eq!(status, StatusCode::OK, "cancel failed: {moved}");
    assert_eq!(moved["status"], json!("cancelled"));
    let (status, moved) = put(&h.app, &h.token, &uri, json!({ "status": "fulfilled" })).await;
    assert_eq!(status, StatusCode::OK, "fulfil failed: {moved}");
    assert_eq!(moved["status"], json!("fulfilled"));

    // A word that is not one of the four is refused, naming them.
    let (status, refused) = put(&h.app, &h.token, &uri, json!({ "status": "posted" })).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{refused}");
    let detail = refused["detail"].as_str().unwrap_or_default().to_owned();
    for word in ["new", "confirmed", "fulfilled", "cancelled"] {
        assert!(
            detail.contains(word),
            "refusal did not name {word}: {detail}"
        );
    }
    // A body that is not the shape is a 400, not a silent default.
    let (status, refused) = put(&h.app, &h.token, &uri, json!({ "state": "new" })).await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{refused}");
    let (status, still) = get(&h.app, &h.token, &format!("/sites/{site}/orders")).await;
    assert_eq!(status, StatusCode::OK, "{still}");
    assert_eq!(still["orders"][1]["status"], json!("fulfilled"));

    // Deleting takes the lines with it, and a second delete is a 404 — the
    // screen's optimistic row is gone, not half-gone.
    let (status, gone) = delete(&h.app, &h.token, &uri).await;
    assert_eq!(status, StatusCode::NO_CONTENT, "{gone}");
    let (status, gone) = delete(&h.app, &h.token, &uri).await;
    assert_eq!(status, StatusCode::NOT_FOUND, "{gone}");
    let (status, left) = get(&h.app, &h.token, &format!("/sites/{site}/orders")).await;
    assert_eq!(status, StatusCode::OK, "{left}");
    let left = left["orders"].as_array().unwrap();
    assert_eq!(left.len(), 1);
    assert_eq!(left[0]["id"], json!(second.as_str()));
    assert!(
        h.acc
            .site_order_lines(&site, &first)
            .await
            .unwrap()
            .is_empty(),
        "the deleted order left its lines behind"
    );
}

/// The export is one row per ordered line, so an owner can sum it, and the
/// visitor's prose can never be evaluated as a spreadsheet formula.
#[tokio::test]
async fn the_export_is_one_row_per_line_and_safe_to_open() {
    let (h, blobs) = harness_with_blobs("sites-orders-csv").await;
    let subdomain = sub("ordersexp", &h);
    let (site, catalog, _) = orderable_site(&h.acc, &subdomain).await;
    let public = public_store(blobs).await;
    let placed = place(
        &public,
        &catalog,
        "Ada Lovelace",
        &[asked("sourdough", 2), asked("wedding-cake", 1)],
    )
    .await;

    let (status, headers, csv) =
        get_csv(&h.app, &h.token, &format!("/sites/{site}/orders.csv")).await;
    assert_eq!(status, StatusCode::OK, "{csv}");
    assert_eq!(
        headers
            .get("content-type")
            .and_then(|value| value.to_str().ok()),
        Some("text/csv; charset=utf-8")
    );
    let expected = format!("attachment; filename=\"orders-{subdomain}.csv\"");
    assert_eq!(
        headers
            .get("content-disposition")
            .and_then(|value| value.to_str().ok()),
        Some(expected.as_str())
    );
    assert_eq!(
        headers
            .get("cache-control")
            .and_then(|value| value.to_str().ok()),
        Some("no-store")
    );
    let rows: Vec<&str> = csv.lines().collect();
    assert_eq!(rows.len(), 3, "header plus one row per line: {csv}");
    assert!(
        rows[0].starts_with("receivedAt,orderId,status,customerName"),
        "{csv}"
    );
    assert!(rows[1].contains(placed.as_str()), "{csv}");
    assert!(rows[1].contains("Sourdough"), "{csv}");
    assert!(rows[1].contains(",2,450,900,900,EUR"), "{csv}");
    // The unpriced line carries empty price columns and still repeats the
    // order's own total, which the priced line alone accounts for.
    assert!(rows[2].contains("Wedding cake"), "{csv}");
    assert!(rows[2].contains(",1,,,900,EUR"), "{csv}");
    assert!(
        csv.contains("'=leave at the door"),
        "a note that looks like a formula reached the cell unneutralised: {csv}"
    );
}

/// Every verb of the inbox is behind the account door.
#[tokio::test]
async fn every_order_route_refuses_an_unauthenticated_caller() {
    let (h, blobs) = harness_with_blobs("sites-orders-401").await;
    let (site, catalog, _) = orderable_site(&h.acc, &sub("orders401", &h)).await;
    let public = public_store(blobs).await;
    let placed = place(&public, &catalog, "Ada Lovelace", &[asked("sourdough", 1)]).await;

    for (method, uri, body) in [
        ("GET", format!("/sites/{site}/orders"), None),
        ("GET", format!("/sites/{site}/orders.csv"), None),
        (
            "PUT",
            format!("/sites/{site}/orders/{placed}"),
            Some(json!({ "status": "confirmed" })),
        ),
        ("DELETE", format!("/sites/{site}/orders/{placed}"), None),
    ] {
        let mut req = Request::builder().method(method).uri(&uri);
        if body.is_some() {
            req = req.header("content-type", "application/json");
        }
        let req = req
            .body(body.map_or_else(Body::empty, |value| Body::from(value.to_string())))
            .unwrap();
        let (status, refused) = send(&h.app, req).await;
        assert_eq!(
            status,
            StatusCode::UNAUTHORIZED,
            "{method} {uri}: {refused}"
        );
    }
    // Nothing was touched by the anonymous attempts.
    let orders = h.acc.site_orders(&site).await.unwrap();
    assert_eq!(orders.len(), 1);
    assert_eq!(orders[0].status.as_str(), "new");
}

/// An order carries a member of the public's name, address and phone number.
/// To a second tenant it must be indistinguishable from one that never
/// existed, on every verb, and nothing it tries may change the owner's rows.
#[tokio::test]
async fn a_foreign_tenant_can_neither_read_nor_touch_another_tenants_orders() {
    let (owner, blobs) = harness_with_blobs("sites-orders-owner").await;
    let rival = harness_on(Arc::clone(&owner.store), "sites-orders-rival").await;
    let (site, catalog, _) = orderable_site(&owner.acc, &sub("ordersown", &owner)).await;
    let public = public_store(blobs).await;
    let placed = place(&public, &catalog, "Ada Lovelace", &[asked("sourdough", 2)]).await;

    let (status, hidden) = get(&rival.app, &rival.token, &format!("/sites/{site}/orders")).await;
    assert_eq!(status, StatusCode::NOT_FOUND, "{hidden}");
    let (status, _, hidden) = get_csv(
        &rival.app,
        &rival.token,
        &format!("/sites/{site}/orders.csv"),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND, "{hidden}");
    let uri = format!("/sites/{site}/orders/{placed}");
    let (status, hidden) = put(
        &rival.app,
        &rival.token,
        &uri,
        json!({ "status": "cancelled" }),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND, "{hidden}");
    let (status, hidden) = delete(&rival.app, &rival.token, &uri).await;
    assert_eq!(status, StatusCode::NOT_FOUND, "{hidden}");

    // The owner's order is exactly as the visitor left it.
    let orders = owner.acc.site_orders(&site).await.unwrap();
    assert_eq!(orders.len(), 1);
    assert_eq!(orders[0].id.as_str(), placed.as_str());
    assert_eq!(orders[0].status.as_str(), "new");
    assert_eq!(orders[0].total_cents, 900);
    assert_eq!(
        owner
            .acc
            .site_order_lines(&site, &placed)
            .await
            .unwrap()
            .len(),
        1
    );
}
