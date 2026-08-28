//! The `/insights/*` HTTP surface (BI1.04) — boards, tiles and the figures they
//! ask for, driven through the real router over a real Postgres.
//!
//! `alo-store`'s own suites prove the records and the query engine work; what
//! this suite is for is the **edge**: that the arc a person walks — make a
//! board, pin a question, read the answer, rearrange, unpin — comes back over
//! the wire with the status codes `docs/design/insights.md` publishes; that the
//! figures on a tile are the same figures the invoice underneath it carries;
//! that a question outside the catalog is a named `422` and never an empty
//! chart; that a tile written by a newer build still renders; and above all
//! that another tenant's board, tile and rows are invisible and untouchable on
//! every verb — **a spec is not a capability**.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use crate::common;

use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use serde_json::{Value, json};
use sqlx::postgres::PgPoolOptions;
use time::OffsetDateTime;

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
    let req = Request::builder()
        .uri(uri)
        .header("authorization", format!("Bearer {token}"))
        .body(Body::empty())
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

fn created_id(kind: &str, (status, body): (StatusCode, Value)) -> String {
    assert_eq!(status, StatusCode::OK, "create failed: {body}");
    body[kind]["id"]
        .as_str()
        .unwrap_or_else(|| panic!("no {kind} id in {body}"))
        .to_owned()
}

// ---- fixtures ----------------------------------------------------------------

/// Net revenue over everything the tenant has, as one figure.
fn revenue_total() -> Value {
    json!({
        "schema_version": 1,
        "dataset": "billing.documents",
        "measure": { "id": "net", "agg": "sum" },
        "period": { "kind": "all" },
        "viz": "number"
    })
}

/// The same money, broken down by the month it was invoiced in.
fn revenue_by_month() -> Value {
    json!({
        "schema_version": 1,
        "dataset": "billing.documents",
        "measure": { "id": "net", "agg": "sum" },
        "dimension": { "id": "issue_date", "grain": "month" },
        "period": { "kind": "last_n", "n": 12, "grain": "month" },
        "viz": "bar"
    })
}

/// A board with a name.
async fn a_board(app: &Router, token: &str, name: &str) -> String {
    created_id(
        "dashboard",
        post(app, token, "/insights/dashboards", json!({ "name": name })).await,
    )
}

/// A tile on `board`, asking `spec`.
async fn a_tile(app: &Router, token: &str, board: &str, title: &str, spec: Value) -> String {
    created_id(
        "tile",
        post(
            app,
            token,
            &format!("/insights/dashboards/{board}/tiles"),
            json!({ "title": title, "spec": spec, "span": 2 }),
        )
        .await,
    )
}

/// Issues one invoice for a new customer, and answers its net in cents.
///
/// The whole point of driving it through `/billing` rather than seeding rows:
/// the figure a tile reports has to be the figure the document carries, and the
/// only way to know they agree is to raise the document the way a bookkeeper
/// does and then ask the chart.
async fn an_issued_invoice(app: &Router, token: &str, customer_name: &str, price: i64) -> i64 {
    let customer = created_id(
        "customer",
        post(
            app,
            token,
            "/billing/customers",
            json!({
                "name": customer_name,
                "addressLine1": "Hauptstraße 1",
                "postalCode": "10115",
                "city": "Berlin",
                "country": "DE",
                "paymentTermsDays": 14,
                "currency": "EUR",
            }),
        )
        .await,
    );
    let (status, body) = post(
        app,
        token,
        "/billing/invoices",
        json!({ "customerId": customer, "lines": [
            { "description": "Consulting", "unit": "hour", "qtyMilli": 2_000,
              "unitPriceCents": price, "vatRateBp": 2_100 },
        ] }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let id = body["invoice"]["id"].as_str().unwrap().to_owned();
    let net = body["invoice"]["totals"]["netCents"].as_i64().unwrap();
    let (status, body) = post(
        app,
        token,
        &format!("/billing/invoices/{id}/issue"),
        json!({}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    net
}

/// The month a document issued today falls in, as the chart spells it.
fn this_month() -> String {
    let today = OffsetDateTime::now_utc().date();
    format!("{:04}-{:02}", today.year(), u8::from(today.month()))
}

/// The single figure a number series carries.
fn only_figure(body: &Value) -> i64 {
    let points = body["series"][0]["points"].as_array().unwrap_or_else(|| {
        panic!("no points in {body}");
    });
    assert_eq!(points.len(), 1, "a number tile has one point: {body}");
    points[0]["value"].as_i64().unwrap_or_else(|| {
        panic!("no value in {body}");
    })
}

// ---- the arc (the item's done-when) -----------------------------------------

#[tokio::test]
async fn the_board_tile_and_answer_arc_runs_on_the_wire() {
    let h = harness("insights-arc").await;
    common::seed_default_chart(&h.acc).await;

    // A tenant's first read is handed the Business overview (BI1.06) and
    // nothing else; the rest of this arc is about the boards a person makes.
    let (status, body) = get(&h.app, &h.token, "/insights/dashboards").await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["dashboards"].as_array().map(Vec::len), Some(1));
    assert_eq!(body["dashboards"][0]["systemKey"], "business_overview");

    // --- a board -------------------------------------------------------------
    let (status, body) = post(
        &h.app,
        &h.token,
        "/insights/dashboards",
        json!({ "name": "  Cash  " }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let board = body["dashboard"]["id"].as_str().unwrap().to_owned();
    assert_eq!(body["dashboard"]["name"], "Cash", "trimmed by the store");
    assert_eq!(body["dashboard"]["seeded"], false);
    assert_eq!(body["dashboard"]["systemKey"], Value::Null);

    // --- a question pinned to it ---------------------------------------------
    let (status, body) = post(
        &h.app,
        &h.token,
        &format!("/insights/dashboards/{board}/tiles"),
        json!({ "title": "Revenue", "spec": revenue_total(), "span": 2 }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let tile = body["tile"]["id"].as_str().unwrap().to_owned();
    assert_eq!(body["tile"]["title"], "Revenue");
    assert_eq!(body["tile"]["span"], 2);
    assert_eq!(body["tile"]["readable"], true);
    assert_eq!(body["tile"]["specError"], Value::Null);
    assert_eq!(
        body["tile"]["viz"], "number",
        "the chart form is derived from the spec, never taken from the caller"
    );
    assert_eq!(
        body["tile"]["spec"]["dataset"], "billing.documents",
        "what comes back is the stored, canonical envelope"
    );

    // --- the board and its tiles in one read ---------------------------------
    let second = a_tile(
        &h.app,
        &h.token,
        &board,
        "Revenue by month",
        revenue_by_month(),
    )
    .await;
    let (status, body) = get(&h.app, &h.token, &format!("/insights/dashboards/{board}")).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["dashboard"]["id"], board.as_str());
    let tiles = body["tiles"].as_array().unwrap();
    assert_eq!(tiles.len(), 2);
    assert_eq!(tiles[0]["id"], tile.as_str(), "layout order, not id order");
    assert_eq!(tiles[1]["id"], second.as_str());
    assert!(
        tiles.iter().all(|t| t.get("value").is_none()),
        "a tile holds a question; the figures are their own read"
    );

    // --- the answer, and the document underneath it --------------------------
    // Nothing has been billed yet, so the honest answer is zero, not silence.
    let (status, body) = get(&h.app, &h.token, &format!("/insights/tiles/{tile}/data")).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["unit"], json!({ "kind": "money", "currency": "EUR" }));
    assert_eq!(only_figure(&body), 0);

    let net = an_issued_invoice(&h.app, &h.token, "Acme GmbH", 12_500).await;
    assert_eq!(net, 25_000, "the document's own arithmetic");
    let (status, body) = get(&h.app, &h.token, &format!("/insights/tiles/{tile}/data")).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(
        only_figure(&body),
        net,
        "the tile reports the document's figure, to the cent"
    );
    assert_eq!(body["truncated"], false);
    assert_eq!(body["notes"], json!([]));

    // The breakdown lands in the month the invoice was issued in, and the
    // quiet months inside the window are zeros rather than gaps.
    let (status, body) = get(&h.app, &h.token, &format!("/insights/tiles/{second}/data")).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let points = body["series"][0]["points"].as_array().unwrap();
    assert_eq!(points.len(), 12, "twelve months asked for, twelve answered");
    let month = points
        .iter()
        .find(|p| p["bucket"] == this_month().as_str())
        .unwrap_or_else(|| panic!("no bucket for {} in {body}", this_month()));
    assert_eq!(month["value"].as_i64(), Some(net));
    assert!(
        month.get("label").is_none(),
        "a time bucket carries no label: its ISO key already says everything"
    );

    // --- the same question asked ad hoc, storing nothing ---------------------
    let (status, body) = post(
        &h.app,
        &h.token,
        "/insights/eval",
        json!({ "spec": revenue_total() }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(only_figure(&body), net, "the preview and the tile agree");
    let (_, body) = get(&h.app, &h.token, &format!("/insights/dashboards/{board}")).await;
    assert_eq!(
        body["tiles"].as_array().map(Vec::len),
        Some(2),
        "a preview pins nothing"
    );

    // --- rearranging, and the two writes that must never be confused ---------
    let (status, body) = patch(
        &h.app,
        &h.token,
        &format!("/insights/tiles/{tile}"),
        json!({ "title": "Revenue, all time", "span": 4 }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["tile"]["title"], "Revenue, all time");
    assert_eq!(body["tile"]["span"], 4);
    assert_eq!(
        body["tile"]["position"], 1.0,
        "an edit does not move a tile"
    );
    assert_eq!(
        body["tile"]["spec"]["measure"]["id"], "net",
        "an edit that says nothing about the question leaves it alone"
    );

    let (status, body) = post(
        &h.app,
        &h.token,
        &format!("/insights/tiles/{tile}/move"),
        json!({ "position": 3.0 }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["tile"]["position"], 3.0);
    assert_eq!(
        body["tile"]["title"], "Revenue, all time",
        "a drag does not retitle a chart"
    );
    let (_, body) = get(&h.app, &h.token, &format!("/insights/dashboards/{board}")).await;
    assert_eq!(
        body["tiles"]
            .as_array()
            .unwrap()
            .iter()
            .map(|t| t["id"].as_str().unwrap_or_default().to_owned())
            .collect::<Vec<_>>(),
        vec![second.clone(), tile.clone()],
        "the board reads back in its new order"
    );

    // --- renaming the board, unpinning, and the cascade ----------------------
    let (status, body) = patch(
        &h.app,
        &h.token,
        &format!("/insights/dashboards/{board}"),
        json!({ "name": "Cash 2027" }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["dashboard"]["name"], "Cash 2027");

    let (status, body) = delete(&h.app, &h.token, &format!("/insights/tiles/{tile}")).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["deleted"], true);
    let (_, body) = get(&h.app, &h.token, &format!("/insights/dashboards/{board}")).await;
    assert_eq!(body["tiles"].as_array().map(Vec::len), Some(1));

    let (status, body) = delete(&h.app, &h.token, &format!("/insights/dashboards/{board}")).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let (status, body) = get(&h.app, &h.token, &format!("/insights/tiles/{second}/data")).await;
    assert_eq!(
        status,
        StatusCode::NOT_FOUND,
        "deleting a board takes its tiles with it: {body}"
    );
    // And the invoice the charts were drawn from is exactly where it was: a
    // dashboard is a view of records, never a record of anything.
    let (status, body) = get(&h.app, &h.token, "/billing/invoices").await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["invoices"].as_array().map(Vec::len), Some(1));
}

// ---- the zero-setup overview and the gallery (BI1.06) ------------------------

/// The item's done-when, end to end: a tenant with books opens Insights and
/// sees live numbers without a single click — the board, its tiles and their
/// figures, in the language of the client that asked.
#[tokio::test]
async fn a_first_visit_answers_with_the_business_overview_and_live_numbers() {
    let h = harness("insights-seed").await;
    common::seed_default_chart(&h.acc).await;
    let net = an_issued_invoice(&h.app, &h.token, "Acme GmbH", 12_500).await;

    // --- one board, nobody asked for it, and it is in French ----------------
    let (status, body) = get(&h.app, &h.token, "/insights/dashboards?lang=fr-BE").await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let boards = body["dashboards"].as_array().unwrap();
    assert_eq!(boards.len(), 1, "{body}");
    assert_eq!(boards[0]["seeded"], true);
    assert_eq!(boards[0]["systemKey"], "business_overview");
    assert_eq!(boards[0]["name"], "Aperçu de l’activité");
    let board = boards[0]["id"].as_str().unwrap().to_owned();

    // --- the tiles, in layout order, each a question this build can read -----
    let (status, body) = get(&h.app, &h.token, &format!("/insights/dashboards/{board}")).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let tiles = body["tiles"].as_array().unwrap().clone();
    assert_eq!(tiles.len(), 7, "the whole overview: {body}");
    assert_eq!(tiles[0]["title"], "Créances en cours");
    assert_eq!(tiles[0]["viz"], "number");
    assert!(
        tiles.iter().all(|t| t["readable"] == json!(true)),
        "a prebuilt question the build cannot read is a dead tile: {body}"
    );
    let positions: Vec<f64> = tiles
        .iter()
        .map(|t| t["position"].as_f64().unwrap_or_default())
        .collect();
    assert!(positions.windows(2).all(|w| w[0] < w[1]), "{positions:?}");

    // --- every tile answers, and the revenue tile answers the document ------
    let mut revenue_seen = false;
    for tile in &tiles {
        let id = tile["id"].as_str().unwrap();
        let (status, figures) = get(&h.app, &h.token, &format!("/insights/tiles/{id}/data")).await;
        assert_eq!(status, StatusCode::OK, "{} → {figures}", tile["title"]);
        if tile["spec"]["measure"]["id"] == "net" {
            revenue_seen = true;
            let month = figures["series"][0]["points"]
                .as_array()
                .unwrap()
                .iter()
                .find(|p| p["bucket"] == this_month().as_str())
                .unwrap_or_else(|| panic!("no bucket for {} in {figures}", this_month()));
            assert_eq!(
                month["value"].as_i64(),
                Some(net),
                "the seeded chart reports the invoice's own figure, to the cent"
            );
        }
    }
    assert!(revenue_seen, "the overview leads with the money");

    // --- a second visit seeds nothing, in any language ----------------------
    let (_, body) = get(&h.app, &h.token, "/insights/dashboards?lang=nl").await;
    assert_eq!(body["dashboards"].as_array().map(Vec::len), Some(1));
    assert_eq!(
        body["dashboards"][0]["name"], "Aperçu de l’activité",
        "the captions are the tenant's own data now, never re-translated"
    );

    // --- and a board thrown away does not come back -------------------------
    let (status, _) = delete(&h.app, &h.token, &format!("/insights/dashboards/{board}")).await;
    assert_eq!(status, StatusCode::OK);
    let (_, body) = get(&h.app, &h.token, "/insights/dashboards").await;
    assert_eq!(
        body["dashboards"].as_array().map(Vec::len),
        Some(0),
        "the seed asks whether it has ever run: {body}"
    );
}

/// The gallery: the prebuilt questions a person pins from. Every one of them is
/// evaluated on this tenant's own rows, and pinning one is the ordinary tile
/// route with the ordinary write gate — never a privileged path.
#[tokio::test]
async fn the_gallery_offers_questions_that_answer_and_pin_like_any_other() {
    let h = harness("insights-gallery").await;
    common::seed_default_chart(&h.acc).await;

    let (status, body) = get(&h.app, &h.token, "/insights/gallery").await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let entries = body["entries"].as_array().unwrap().clone();
    assert!(entries.len() >= 7, "{body}");
    assert!(
        body["overview"]
            .as_array()
            .is_some_and(|keys| !keys.is_empty()),
        "the client is told which entries the zero-setup board is built from"
    );
    for key in body["overview"].as_array().unwrap() {
        assert!(
            entries.iter().any(|e| e["key"] == *key),
            "the overview names {key}, which the gallery does not offer"
        );
    }

    let board = a_board(&h.app, &h.token, "Mine").await;
    for entry in &entries {
        let key = entry["key"].as_str().unwrap();
        assert!(
            entry.get("title").is_none() && entry.get("description").is_none(),
            "no English crosses the wire from the gallery: {entry}"
        );
        assert!(
            ["billing", "crm"].contains(&entry["module"].as_str().unwrap_or_default()),
            "{entry}"
        );

        // It answers, ad hoc, on this tenant's rows.
        let (status, figures) = post(
            &h.app,
            &h.token,
            "/insights/eval",
            json!({ "spec": entry["spec"] }),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{key} → {figures}");

        // And it pins through the same route, and the same gate, as any spec.
        let (status, pinned) = post(
            &h.app,
            &h.token,
            &format!("/insights/dashboards/{board}/tiles"),
            json!({ "title": key, "spec": entry["spec"], "span": entry["span"] }),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{key} → {pinned}");
        assert_eq!(pinned["tile"]["readable"], true, "{key}");
        assert_eq!(pinned["tile"]["viz"], entry["viz"], "{key}");
        assert_eq!(
            pinned["tile"]["spec"], entry["spec"],
            "{key} is stored exactly as the gallery offered it"
        );
    }
}

// ---- the guards --------------------------------------------------------------

#[tokio::test]
async fn every_insights_route_refuses_an_unauthenticated_caller() {
    let h = harness("insights-401").await;
    common::seed_default_chart(&h.acc).await;
    let board = a_board(&h.app, &h.token, "Cash").await;
    let tile = a_tile(&h.app, &h.token, &board, "Revenue", revenue_total()).await;

    let mut attempts: Vec<Request<Body>> = vec![
        with_json("POST", "/insights/dashboards", None, json!({ "name": "x" })),
        with_json(
            "PATCH",
            &format!("/insights/dashboards/{board}"),
            None,
            json!({ "name": "x" }),
        ),
        with_json(
            "POST",
            &format!("/insights/dashboards/{board}/tiles"),
            None,
            json!({ "title": "x", "spec": revenue_total() }),
        ),
        with_json(
            "PATCH",
            &format!("/insights/tiles/{tile}"),
            None,
            json!({ "title": "x" }),
        ),
        with_json(
            "POST",
            &format!("/insights/tiles/{tile}/move"),
            None,
            json!({ "position": 2.0 }),
        ),
        with_json(
            "POST",
            "/insights/eval",
            None,
            json!({ "spec": revenue_total() }),
        ),
    ];
    for uri in [
        "/insights/dashboards".to_owned(),
        "/insights/gallery".to_owned(),
        format!("/insights/dashboards/{board}"),
        format!("/insights/tiles/{tile}/data"),
    ] {
        attempts.push(Request::builder().uri(uri).body(Body::empty()).unwrap());
    }
    for uri in [
        format!("/insights/dashboards/{board}"),
        format!("/insights/tiles/{tile}"),
    ] {
        attempts.push(
            Request::builder()
                .method("DELETE")
                .uri(uri)
                .body(Body::empty())
                .unwrap(),
        );
    }

    for req in attempts {
        let uri = req.uri().to_string();
        let method = req.method().to_string();
        let (status, _) = send(&h.app, req).await;
        assert_eq!(status, StatusCode::UNAUTHORIZED, "{method} {uri}");
    }

    // Nothing an unauthenticated caller tried has changed anything.
    let (_, body) = get(&h.app, &h.token, &format!("/insights/dashboards/{board}")).await;
    assert_eq!(body["dashboard"]["name"], "Cash");
    assert_eq!(body["tiles"].as_array().map(Vec::len), Some(1));
    assert_eq!(body["tiles"][0]["title"], "Revenue");
}

#[tokio::test]
async fn a_question_outside_the_catalog_is_a_named_422_and_never_an_empty_chart() {
    let h = harness("insights-422").await;
    common::seed_default_chart(&h.acc).await;
    let board = a_board(&h.app, &h.token, "Cash").await;

    // Every one of these is a spec a builder UI — or a model on its one repair
    // attempt — has to be told the truth about, on both the routes that take a
    // spec: the preview and the pin.
    let broken: Vec<(&str, Value)> = vec![
        ("an invented measure", {
            let mut spec = revenue_total();
            spec["measure"] = json!({ "id": "profit", "agg": "sum" });
            spec
        }),
        ("a pairing the catalog does not declare", {
            let mut spec = revenue_total();
            spec["measure"] = json!({ "id": "count", "agg": "count" });
            spec["dimension"] = json!({ "id": "vat_rate" });
            spec["viz"] = json!("bar");
            spec
        }),
        ("a grain a dimension does not allow", {
            let mut spec = revenue_by_month();
            spec["dataset"] = json!("crm.deals");
            spec["measure"] = json!({ "id": "value", "agg": "sum" });
            spec["dimension"] = json!({ "id": "created_at", "grain": "day" });
            spec
        }),
        ("a chart form that disagrees with the breakdown", {
            let mut spec = revenue_total();
            spec["dimension"] = json!({ "id": "customer" });
            spec
        }),
        ("a bound exceeded", {
            let mut spec = revenue_by_month();
            spec["limit"] = json!(500);
            spec
        }),
        ("a schema version this build does not speak", {
            let mut spec = revenue_total();
            spec["schema_version"] = json!(2);
            spec
        }),
        ("a field the envelope has no room for", {
            let mut spec = revenue_total();
            spec["colour"] = json!("blue");
            spec
        }),
        ("a period field that belongs to another kind", {
            let mut spec = revenue_total();
            spec["period"] = json!({ "kind": "all", "n": 3 });
            spec
        }),
    ];
    for (what, spec) in broken {
        let (status, body) = post(
            &h.app,
            &h.token,
            "/insights/eval",
            json!({ "spec": spec.clone() }),
        )
        .await;
        assert_eq!(
            status,
            StatusCode::UNPROCESSABLE_ENTITY,
            "eval accepted {what}: {body}"
        );
        assert!(
            body["detail"].as_str().is_some_and(|d| !d.is_empty()),
            "a refusal a caller cannot act on is no better than an empty chart: {body}"
        );
        let (status, body) = post(
            &h.app,
            &h.token,
            &format!("/insights/dashboards/{board}/tiles"),
            json!({ "title": "Broken", "spec": spec }),
        )
        .await;
        assert_eq!(
            status,
            StatusCode::UNPROCESSABLE_ENTITY,
            "the write gate accepted {what}: {body}"
        );
    }

    // The rest of the edge's own refusals.
    let cases: Vec<(&str, StatusCode, Request<Body>)> = vec![
        (
            "a board with no name",
            StatusCode::UNPROCESSABLE_ENTITY,
            with_json(
                "POST",
                "/insights/dashboards",
                Some(&h.token),
                json!({ "name": "   " }),
            ),
        ),
        (
            "a tile with no question",
            StatusCode::UNPROCESSABLE_ENTITY,
            with_json(
                "POST",
                &format!("/insights/dashboards/{board}/tiles"),
                Some(&h.token),
                json!({ "title": "Empty" }),
            ),
        ),
        (
            "a tile with no caption",
            StatusCode::UNPROCESSABLE_ENTITY,
            with_json(
                "POST",
                &format!("/insights/dashboards/{board}/tiles"),
                Some(&h.token),
                json!({ "spec": revenue_total() }),
            ),
        ),
        (
            "a span outside the grid",
            StatusCode::UNPROCESSABLE_ENTITY,
            with_json(
                "POST",
                &format!("/insights/dashboards/{board}/tiles"),
                Some(&h.token),
                json!({ "title": "Wide", "spec": revenue_total(), "span": 40 }),
            ),
        ),
        (
            "an evaluation with no question at all",
            StatusCode::UNPROCESSABLE_ENTITY,
            with_json("POST", "/insights/eval", Some(&h.token), json!({})),
        ),
        (
            "a body that is not JSON",
            StatusCode::BAD_REQUEST,
            Request::builder()
                .method("POST")
                .uri("/insights/dashboards")
                .header("authorization", format!("Bearer {}", h.token))
                .header("content-type", "application/json")
                .body(Body::from("{not json"))
                .unwrap(),
        ),
    ];
    for (what, expected, req) in cases {
        let (status, body) = send(&h.app, req).await;
        assert_eq!(status, expected, "{what}: {body}");
    }

    // A move that does not say where is not a move.
    let tile = a_tile(&h.app, &h.token, &board, "Revenue", revenue_total()).await;
    let (status, body) = post(
        &h.app,
        &h.token,
        &format!("/insights/tiles/{tile}/move"),
        json!({}),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");

    // And nothing that was refused was written.
    let (_, body) = get(&h.app, &h.token, &format!("/insights/dashboards/{board}")).await;
    assert_eq!(body["tiles"].as_array().map(Vec::len), Some(1));
    assert_eq!(body["tiles"][0]["title"], "Revenue");
    assert_eq!(body["tiles"][0]["position"], 1.0);
}

#[tokio::test]
async fn a_tile_from_the_future_still_renders_and_says_why_it_cannot_answer() {
    let h = harness("insights-future").await;
    common::seed_default_chart(&h.acc).await;
    let board = a_board(&h.app, &h.token, "Cash").await;
    let ok = a_tile(&h.app, &h.token, &board, "Revenue", revenue_total()).await;
    let future = a_tile(&h.app, &h.token, &board, "Later", revenue_by_month()).await;

    // What a newer build might have written, planted underneath the API — the
    // write gate cannot produce it, which is the whole point of the read being
    // tolerant.
    let pool = PgPoolOptions::new()
        .max_connections(1)
        .connect(&database_url())
        .await
        .expect("connect");
    sqlx::query(
        "UPDATE insight_tiles SET spec = $3, viz = 'sankey' WHERE tenant_id = $1 AND id = $2",
    )
    .bind(h.tenant.as_str())
    .bind(&future)
    .bind(sqlx::types::Json(
        json!({ "schema_version": 2, "dataset": "billing.documents" }),
    ))
    .execute(&pool)
    .await
    .expect("plant a spec from the future");

    // The board renders: one tile from the future does not break the read.
    let (status, body) = get(&h.app, &h.token, &format!("/insights/dashboards/{board}")).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let tiles = body["tiles"].as_array().unwrap();
    assert_eq!(tiles.len(), 2);
    assert_eq!(tiles[0]["id"], ok.as_str());
    assert_eq!(tiles[0]["readable"], true);
    assert_eq!(tiles[1]["id"], future.as_str());
    assert_eq!(tiles[1]["readable"], false);
    assert_eq!(
        tiles[1]["viz"],
        Value::Null,
        "an unknown form is not guessed"
    );
    assert_eq!(
        tiles[1]["spec"]["schema_version"], 2,
        "handed back untouched"
    );
    assert!(
        tiles[1]["specError"]
            .as_str()
            .is_some_and(|r| r.contains("schema_version")),
        "the placeholder can say why: {body}"
    );

    // Asking for its figures is the one thing that cannot be answered honestly.
    let (status, body) = get(&h.app, &h.token, &format!("/insights/tiles/{future}/data")).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
    // Nor can it be half-edited: a caption change has no readable question to
    // merge onto, and the refusal names the way out.
    let (status, body) = patch(
        &h.app,
        &h.token,
        &format!("/insights/tiles/{future}"),
        json!({ "title": "Renamed" }),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
    assert!(
        body["detail"]
            .as_str()
            .is_some_and(|d| d.contains("send a spec")),
        "{body}"
    );

    // Replacing the question heals the tile, in place, keeping its position.
    let (status, body) = patch(
        &h.app,
        &h.token,
        &format!("/insights/tiles/{future}"),
        json!({ "title": "Revenue by month", "spec": revenue_by_month() }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["tile"]["readable"], true);
    assert_eq!(body["tile"]["viz"], "bar");
    assert_eq!(body["tile"]["position"], 2.0);
    let (status, _) = get(&h.app, &h.token, &format!("/insights/tiles/{future}/data")).await;
    assert_eq!(status, StatusCode::OK);
}

// ---- the wrong-tenant test (mandatory: CLAUDE.md law 1) ----------------------

#[tokio::test]
async fn another_tenants_board_tile_and_figures_are_out_of_reach_on_every_route() {
    let a = harness("insights-tenant-a").await;
    common::seed_default_chart(&a.acc).await;
    let b = harness("insights-tenant-b").await;
    common::seed_default_chart(&b.acc).await;

    // Two tenants, different books: A billed 25 000, B billed 90 000.
    let a_net = an_issued_invoice(&a.app, &a.token, "Acme GmbH", 12_500).await;
    let b_net = an_issued_invoice(&b.app, &b.token, "Beta BV", 45_000).await;
    assert_ne!(a_net, b_net);

    let b_board = a_board(&b.app, &b.token, "B's cash").await;
    let b_tile = a_tile(&b.app, &b.token, &b_board, "B's revenue", revenue_total()).await;

    // A's own list never shows B's board — only A's own, and the overview A's
    // first read seeded for A alone.
    let a_board_id = a_board(&a.app, &a.token, "A's cash").await;
    let (_, body) = get(&a.app, &a.token, "/insights/dashboards").await;
    let a_boards: Vec<String> = body["dashboards"]
        .as_array()
        .unwrap()
        .iter()
        .map(|d| d["id"].as_str().unwrap_or_default().to_owned())
        .collect();
    assert!(a_boards.contains(&a_board_id), "{body}");
    assert!(!a_boards.contains(&b_board), "{body}");
    assert_eq!(a_boards.len(), 2, "A's own board and A's own overview");

    // **A spec is not a capability.** The very same question, asked by each
    // tenant, answers that tenant's own books and nobody else's.
    for (h, expected) in [(&a, a_net), (&b, b_net)] {
        let (status, body) = post(
            &h.app,
            &h.token,
            "/insights/eval",
            json!({ "spec": revenue_total() }),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{body}");
        assert_eq!(only_figure(&body), expected, "{body}");
    }

    // Every verb on B's ids answers A with the same 404 an invented id gets.
    let invented_board = "dash_does_not_exist";
    let invented_tile = "tile_does_not_exist";
    let attempts: Vec<(&str, String, Option<Value>)> = vec![
        ("GET", format!("/insights/dashboards/{b_board}"), None),
        (
            "PATCH",
            format!("/insights/dashboards/{b_board}"),
            Some(json!({ "name": "Taken Over" })),
        ),
        ("DELETE", format!("/insights/dashboards/{b_board}"), None),
        (
            "POST",
            format!("/insights/dashboards/{b_board}/tiles"),
            Some(json!({ "title": "Ours now", "spec": revenue_total() })),
        ),
        (
            "PATCH",
            format!("/insights/tiles/{b_tile}"),
            Some(json!({ "title": "Taken Over" })),
        ),
        (
            "POST",
            format!("/insights/tiles/{b_tile}/move"),
            Some(json!({ "position": 99.0 })),
        ),
        ("GET", format!("/insights/tiles/{b_tile}/data"), None),
        ("DELETE", format!("/insights/tiles/{b_tile}"), None),
        // The same verbs against ids that never existed anywhere: the answers
        // must be indistinguishable, or the surface is an existence oracle.
        (
            "GET",
            format!("/insights/dashboards/{invented_board}"),
            None,
        ),
        (
            "PATCH",
            format!("/insights/dashboards/{invented_board}"),
            Some(json!({ "name": "Taken Over" })),
        ),
        (
            "DELETE",
            format!("/insights/dashboards/{invented_board}"),
            None,
        ),
        (
            "POST",
            format!("/insights/dashboards/{invented_board}/tiles"),
            Some(json!({ "title": "Nowhere", "spec": revenue_total() })),
        ),
        (
            "PATCH",
            format!("/insights/tiles/{invented_tile}"),
            Some(json!({ "title": "Taken Over" })),
        ),
        (
            "POST",
            format!("/insights/tiles/{invented_tile}/move"),
            Some(json!({ "position": 99.0 })),
        ),
        ("GET", format!("/insights/tiles/{invented_tile}/data"), None),
        ("DELETE", format!("/insights/tiles/{invented_tile}"), None),
    ];
    for (method, uri, body) in attempts {
        let req = match body {
            Some(json) => with_json(method, &uri, Some(&a.token), json),
            None => Request::builder()
                .method(method)
                .uri(&uri)
                .header("authorization", format!("Bearer {}", a.token))
                .body(Body::empty())
                .unwrap(),
        };
        let (status, answer) = send(&a.app, req).await;
        assert_eq!(status, StatusCode::NOT_FOUND, "{method} {uri}: {answer}");
    }

    // A filter naming B's customer is refused rather than answered with an
    // empty chart — a silently empty tile is how a business comes to believe it
    // billed nothing last quarter, and it must not double as an oracle for
    // whether an id exists somewhere else.
    let (_, b_customers) = get(&b.app, &b.token, "/billing/customers").await;
    let b_customer = b_customers["customers"][0]["id"].as_str().unwrap();
    for id in [b_customer, "cust_does_not_exist"] {
        let mut spec = revenue_total();
        spec["filters"] = json!([{ "id": "customer", "op": "in", "values": [id] }]);
        let (status, body) =
            post(&a.app, &a.token, "/insights/eval", json!({ "spec": spec })).await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{id}: {body}");
    }

    // B's records are untouched by every one of those attempts.
    let (status, body) = get(&b.app, &b.token, &format!("/insights/dashboards/{b_board}")).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["dashboard"]["name"], "B's cash");
    assert_eq!(body["tiles"].as_array().map(Vec::len), Some(1));
    assert_eq!(body["tiles"][0]["title"], "B's revenue");
    assert_eq!(body["tiles"][0]["position"], 1.0);
    let (status, body) = get(&b.app, &b.token, &format!("/insights/tiles/{b_tile}/data")).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(only_figure(&body), b_net);
}
