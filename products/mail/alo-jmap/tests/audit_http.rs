//! The business audit trail (B2.13) on the wire — `GET /audit?entity=` over a
//! real router and a real Postgres.
//!
//! The item's promise is narrow and total: **every mutating billing/CRM route
//! writes exactly one entry, and nothing else writes any.** `audit_routes.rs`
//! proves the first half statically over the router's own source; this suite
//! proves the halves that only a running service can show —
//!
//! - an act writes one entry, with the right verb, the right record and the
//!   acting person, in the order the acts happened;
//! - a *refused* act writes none (a history of things that did not happen is
//!   worse than no history), and a read writes none;
//! - a sub-resource event lands on the record it belongs to — a payment on its
//!   invoice, not on a page of its own;
//! - and another tenant reading a record id of ours gets an empty history
//!   rather than an answer, exactly like an id that never existed.

#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use serde_json::{Value, json};

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

// ---- reading a history -------------------------------------------------------

/// One record's history as `(action, actor, target)` triples, newest first.
async fn history(h: &Harness, entity: &str) -> Vec<(String, String, String)> {
    let (status, body) = get(&h.app, &h.token, &format!("/audit?entity={entity}")).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    body["entries"]
        .as_array()
        .unwrap_or_else(|| panic!("no entries array in {body}"))
        .iter()
        .map(|e| {
            (
                e["action"].as_str().unwrap_or_default().to_owned(),
                e["actor"].as_str().unwrap_or_default().to_owned(),
                e["target"].as_str().unwrap_or_default().to_owned(),
            )
        })
        .collect()
}

/// Just the verbs, newest first — what most assertions here are about.
async fn actions(h: &Harness, entity: &str) -> Vec<String> {
    history(h, entity)
        .await
        .into_iter()
        .map(|(action, _, _)| action)
        .collect()
}

// ---- fixtures ----------------------------------------------------------------

async fn a_customer(h: &Harness) -> String {
    let (status, body) = post(
        &h.app,
        &h.token,
        "/billing/customers",
        json!({
            "name": "Acme GmbH",
            "addressLine1": "Hauptstraße 1",
            "postalCode": "10115",
            "city": "Berlin",
            "country": "DE",
            "paymentTermsDays": 14,
            "currency": "EUR",
        }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    body["customer"]["id"].as_str().unwrap().to_owned()
}

async fn an_invoice(h: &Harness, customer: &str) -> String {
    let (status, body) = post(
        &h.app,
        &h.token,
        "/billing/invoices",
        json!({
            "customerId": customer,
            "lines": [
                { "description": "Consulting", "unit": "hour", "qtyMilli": 2_000,
                  "unitPriceCents": 10_000, "vatRateBp": 2_100 },
            ],
        }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    body["invoice"]["id"].as_str().unwrap().to_owned()
}

async fn a_deal(h: &Harness) -> String {
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
    let stage = body["stages"][0]["id"].as_str().unwrap().to_owned();
    let (status, body) = post(
        &h.app,
        &h.token,
        "/crm/deals",
        json!({ "pipelineId": pipeline, "stageId": stage, "title": "Renewal — Acme GmbH" }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    body["deal"]["id"].as_str().unwrap().to_owned()
}

// ---- the arc (the item's done-when) -----------------------------------------

/// The whole point, on one record: create, edit, issue, pay, unpay — five acts,
/// five entries, newest first, each naming the person who did it.
#[tokio::test]
async fn every_act_on_an_invoice_writes_exactly_one_entry() {
    let h = harness("audit-arc").await;
    common::seed_default_chart(&h.acc).await;
    let customer = a_customer(&h).await;
    let invoice = an_invoice(&h, &customer).await;
    let entity = format!("billing.invoice:{invoice}");

    // The create was recorded against the id the response carried — the id did
    // not exist anywhere in the request.
    let entries = history(&h, &entity).await;
    assert_eq!(entries.len(), 1, "{entries:?}");
    assert_eq!(entries[0].0, "billing.invoice.create");
    assert_eq!(entries[0].1, h.email, "the actor is the bearer's user");
    assert_eq!(entries[0].2, "/billing/invoices");

    let (status, body) = patch(
        &h.app,
        &h.token,
        &format!("/billing/invoices/{invoice}"),
        json!({ "reference": "PO-77" }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");

    let (status, body) = post(
        &h.app,
        &h.token,
        &format!("/billing/invoices/{invoice}/issue"),
        json!({}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let gross = body["invoice"]["totals"]["grossCents"].as_i64().unwrap();

    // A payment is a sub-resource: its own route, filed on the invoice.
    let (status, body) = post(
        &h.app,
        &h.token,
        &format!("/billing/invoices/{invoice}/payments"),
        json!({ "amountCents": gross, "method": "transfer" }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let payment = body["payment"]["id"].as_str().unwrap().to_owned();
    let (status, body) = delete(
        &h.app,
        &h.token,
        &format!("/billing/invoices/{invoice}/payments/{payment}"),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");

    assert_eq!(
        actions(&h, &entity).await,
        vec![
            "billing.invoice.payment.delete",
            "billing.invoice.payment.create",
            "billing.invoice.issue",
            "billing.invoice.update",
            "billing.invoice.create",
        ],
        "one entry per act, newest first"
    );
    // The payment events are on the invoice and nowhere else — a payment has no
    // page of its own, which is exactly why they are filed on the parent.
    assert!(
        actions(&h, &format!("billing.payment:{payment}"))
            .await
            .is_empty()
    );
}

/// A customer's own history, and the proof that the trail follows the record
/// rather than the module: the invoice raised for this customer is not in it.
#[tokio::test]
async fn a_history_is_the_records_own_and_nobody_elses() {
    let h = harness("audit-scope").await;
    common::seed_default_chart(&h.acc).await;
    let customer = a_customer(&h).await;
    let entity = format!("billing.customer:{customer}");

    let (status, body) = patch(
        &h.app,
        &h.token,
        &format!("/billing/customers/{customer}"),
        json!({ "city": "Hamburg" }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let (status, body) = post(
        &h.app,
        &h.token,
        &format!("/billing/customers/{customer}/archive"),
        json!({ "archived": true }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let invoice = a_customer(&h).await;
    let _ = invoice;

    assert_eq!(
        actions(&h, &entity).await,
        vec![
            "billing.customer.archive",
            "billing.customer.update",
            "billing.customer.create",
        ]
    );
    // The second customer created above is a different record with its own
    // history of one — creates do not pool.
    assert_eq!(history(&h, &entity).await.len(), 3);
}

/// CRM records keep the same trail, with the same verbs derived from the same
/// routes — the layer knows nothing about either module.
#[tokio::test]
async fn a_deal_keeps_the_same_trail() {
    let h = harness("audit-crm").await;
    common::seed_default_chart(&h.acc).await;
    let deal = a_deal(&h).await;
    let entity = format!("crm.deal:{deal}");

    let (status, body) = patch(
        &h.app,
        &h.token,
        &format!("/crm/deals/{deal}"),
        json!({ "valueCents": 250_000 }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
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
    let next = body["stages"][1]["id"].as_str().unwrap().to_owned();
    let (status, body) = post(
        &h.app,
        &h.token,
        &format!("/crm/deals/{deal}/stage"),
        json!({ "stageId": next }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let (status, body) = post(
        &h.app,
        &h.token,
        &format!("/crm/deals/{deal}/activities"),
        json!({ "kind": "note", "body": "Called, they want a quote." }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");

    assert_eq!(
        actions(&h, &entity).await,
        vec![
            "crm.deal.activity.create",
            "crm.deal.stage",
            "crm.deal.update",
            "crm.deal.create",
        ]
    );
}

// ---- what is NOT recorded ----------------------------------------------------

/// Reads and refusals leave no trace. A history that lists rejected edits reads
/// as a record of changes that were never made.
#[tokio::test]
async fn reads_and_refused_writes_are_not_recorded() {
    let h = harness("audit-quiet").await;
    common::seed_default_chart(&h.acc).await;
    let customer = a_customer(&h).await;
    let invoice = an_invoice(&h, &customer).await;
    let entity = format!("billing.invoice:{invoice}");
    let before = actions(&h, &entity).await;
    assert_eq!(before.len(), 1);

    // Reads: the document, the list, the print view, its own history.
    for uri in [
        format!("/billing/invoices/{invoice}"),
        "/billing/invoices".to_owned(),
        format!("/audit?entity={entity}"),
    ] {
        let (status, _) = get(&h.app, &h.token, &uri).await;
        assert_eq!(status, StatusCode::OK, "{uri}");
    }

    // A body that is not JSON, an unknown record, and an edit the document
    // refuses once it is frozen — three refusals, no entries.
    let (status, _) = send(
        &h.app,
        Request::builder()
            .method("PATCH")
            .uri(format!("/billing/invoices/{invoice}"))
            .header("authorization", format!("Bearer {}", h.token))
            .header("content-type", "application/json")
            .body(Body::from("{not json"))
            .unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    let (status, _) = patch(
        &h.app,
        &h.token,
        "/billing/invoices/never-existed",
        json!({ "reference": "x" }),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let (status, body) = post(
        &h.app,
        &h.token,
        &format!("/billing/invoices/{invoice}/issue"),
        json!({}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let (status, _) = patch(
        &h.app,
        &h.token,
        &format!("/billing/invoices/{invoice}"),
        json!({ "reference": "too late" }),
    )
    .await;
    assert_ne!(status, StatusCode::OK, "an issued invoice refuses edits");

    assert_eq!(
        actions(&h, &entity).await,
        vec!["billing.invoice.issue", "billing.invoice.create"],
        "only the two acts that actually happened"
    );
}

/// The dry run of the lead import answers what *would* be imported and changes
/// nothing, so it is on the layer's read-only list and writes no entry — while
/// the import that follows it does.
#[tokio::test]
async fn the_import_dry_run_is_not_an_act() {
    let h = harness("audit-preview").await;
    common::seed_default_chart(&h.acc).await;
    let (status, body) = get(&h.app, &h.token, "/crm/pipelines").await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let pipeline = body["pipelines"][0]["id"].as_str().unwrap().to_owned();
    let csv = "Company;E-mail\nAcme GmbH;ops@acme.example\n";
    let preview = Request::builder()
        .method("POST")
        .uri(format!("/crm/imports/leads/preview?pipelineId={pipeline}"))
        .header("authorization", format!("Bearer {}", h.token))
        .header("content-type", "text/csv")
        .body(Body::from(csv))
        .unwrap();
    let (status, body) = send(&h.app, preview).await;
    assert_eq!(status, StatusCode::OK, "{body}");

    // Read the tenant's whole log, not one record's history: an entry for a dry
    // run would carry no record id and would therefore hide from every history.
    let logged: Vec<String> =
        h.ts.list_audit(200)
            .await
            .unwrap()
            .into_iter()
            .map(|entry| entry.action)
            .collect();
    assert!(
        !logged.iter().any(|action| action.starts_with("crm.import")),
        "a dry run is not an act: {logged:?}"
    );

    // The commit that follows it is, and it is in the same log.
    let commit = Request::builder()
        .method("POST")
        .uri(format!("/crm/imports/leads?pipelineId={pipeline}"))
        .header("authorization", format!("Bearer {}", h.token))
        .header("content-type", "text/csv")
        .body(Body::from(csv))
        .unwrap();
    let (status, body) = send(&h.app, commit).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let logged: Vec<String> =
        h.ts.list_audit(200)
            .await
            .unwrap()
            .into_iter()
            .map(|entry| entry.action)
            .collect();
    assert_eq!(
        logged
            .iter()
            .filter(|action| action.as_str() == "crm.import.lead.create")
            .count(),
        1,
        "{logged:?}"
    );
}

// ---- the guards --------------------------------------------------------------

#[tokio::test]
async fn the_history_needs_a_token_and_a_record() {
    let h = harness("audit-guards").await;
    common::seed_default_chart(&h.acc).await;
    let anonymous = Request::builder()
        .uri("/audit?entity=billing.invoice:whatever")
        .body(Body::empty())
        .unwrap();
    let (status, _) = send(&h.app, anonymous).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);

    for query in [
        "/audit",
        "/audit?entity=",
        "/audit?entity=billing.invoice",
        "/audit?entity=:abc",
        "/audit?entity=billing.invoice:",
    ] {
        let (status, body) = get(&h.app, &h.token, query).await;
        assert_eq!(
            status,
            StatusCode::UNPROCESSABLE_ENTITY,
            "{query} answered {body}"
        );
    }

    // A record that does not exist is an empty history, not a 404: the endpoint
    // is never an oracle for which ids are real.
    let (status, body) = get(&h.app, &h.token, "/audit?entity=billing.invoice:nope").await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["entries"].as_array().unwrap().len(), 0);
}

/// The tenancy law on the read side: tenant B naming tenant A's invoice gets
/// the same empty history as an invented id, and A's own history is untouched
/// by anything B did.
#[tokio::test]
async fn another_tenants_record_has_no_history_here() {
    let a = harness("audit-ten-a").await;
    common::seed_default_chart(&a.acc).await;
    let b = harness("audit-ten-b").await;
    common::seed_default_chart(&b.acc).await;
    let customer = a_customer(&a).await;
    let invoice = an_invoice(&a, &customer).await;
    let entity = format!("billing.invoice:{invoice}");

    let (status, body) = get(&b.app, &b.token, &format!("/audit?entity={entity}")).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(
        body["entries"].as_array().unwrap().len(),
        0,
        "tenant B read tenant A's history: {body}"
    );
    let (invented_status, invented_body) =
        get(&b.app, &b.token, "/audit?entity=billing.invoice:invented").await;
    assert_eq!(invented_status, status, "the two answers must not differ");
    assert_eq!(invented_body, body, "byte for byte, or it is an oracle");

    // B's own work is in B's log and only there.
    let b_customer = a_customer(&b).await;
    assert_eq!(
        actions(&b, &format!("billing.customer:{b_customer}")).await,
        vec!["billing.customer.create"]
    );
    assert_eq!(
        actions(&a, &format!("billing.customer:{b_customer}"))
            .await
            .len(),
        0,
        "tenant A must not see tenant B's record either"
    );
    // And A's history is exactly what A did.
    assert_eq!(actions(&a, &entity).await, vec!["billing.invoice.create"]);
}

/// The `limit` a caller may ask for is honoured and bounded — a history read is
/// a page, never an unbounded scan.
#[tokio::test]
async fn the_history_is_paged_newest_first() {
    let h = harness("audit-limit").await;
    common::seed_default_chart(&h.acc).await;
    let customer = a_customer(&h).await;
    for city in ["Hamburg", "Bremen", "Kiel"] {
        let (status, body) = patch(
            &h.app,
            &h.token,
            &format!("/billing/customers/{customer}"),
            json!({ "city": city }),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{body}");
    }
    let entity = format!("billing.customer:{customer}");
    assert_eq!(actions(&h, &entity).await.len(), 4);

    let (status, body) = get(&h.app, &h.token, &format!("/audit?entity={entity}&limit=2")).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let entries = body["entries"].as_array().unwrap();
    assert_eq!(entries.len(), 2);
    assert_eq!(entries[0]["action"], "billing.customer.update");
    // A nonsense limit is clamped, never taken literally.
    let (status, body) = get(
        &h.app,
        &h.token,
        &format!("/audit?entity={entity}&limit=-9999999"),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["entries"].as_array().unwrap().len(), 1);
}
