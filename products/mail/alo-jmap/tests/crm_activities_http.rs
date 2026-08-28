//! A deal's log and its next steps over HTTP (B2.06), driven through the real
//! router over a real Postgres.
//!
//! `alo-store`'s own suite proves the records and the two boundaries; what
//! matters here is the **edge**: the auth guard on every route, the status codes
//! `docs/design/crm.md` publishes — including the one `403` in CRM, where a
//! colleague deleting somebody else's note is told the reason rather than lied
//! to about the row's existence — and that a next step really is a task, in the
//! tasks module's own shape and readable through `/tasks`.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use serde_json::{Value, json};

use crate::common::{Harness, harness, send};

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

/// A deal on the tenant's seeded board.
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
        json!({
            "pipelineId": pipeline,
            "stageId": stage,
            "title": "Renewal — Acme GmbH",
        }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    body["deal"]["id"].as_str().unwrap().to_owned()
}

// ---- the log -----------------------------------------------------------------

#[tokio::test]
async fn an_entry_is_written_read_back_and_removed() {
    let h = harness("crmact-arc").await;
    let deal = a_deal(&h).await;

    let (status, body) = get(&h.app, &h.token, &format!("/crm/deals/{deal}/activities")).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["activities"].as_array().map(Vec::len), Some(0));

    let (status, body) = post(
        &h.app,
        &h.token,
        &format!("/crm/deals/{deal}/activities"),
        json!({
            "kind": "call",
            "body": "  Ada wants 40 seats quoted.  ",
            "happenedAt": "2026-01-07T16:05:00+02:00",
        }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let id = body["activity"]["id"].as_str().unwrap().to_owned();
    assert_eq!(body["activity"]["kind"], "call");
    // The answer is the STORED record: trimmed, dated in UTC, authored by the
    // caller — never an echo of the request.
    assert_eq!(body["activity"]["body"], "Ada wants 40 seats quoted.");
    assert_eq!(body["activity"]["happenedAt"], "2026-01-07T14:05:00Z");
    assert_eq!(body["activity"]["authorUserId"], h.account_id.as_str());
    assert_eq!(body["activity"]["dealId"], deal.as_str());

    // An entry with nothing but a body is a note that happened now — later
    // than the call, which was dated in January.
    let (status, body) = post(
        &h.app,
        &h.token,
        &format!("/crm/deals/{deal}/activities"),
        json!({ "body": "Sent the deck." }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["activity"]["kind"], "note");

    // Newest first, by when it happened.
    let (status, body) = get(&h.app, &h.token, &format!("/crm/deals/{deal}/activities")).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["activities"].as_array().map(Vec::len), Some(2));
    assert_eq!(body["activities"][0]["body"], "Sent the deck.");
    assert_eq!(body["activities"][1]["id"], id.as_str());

    let (status, body) = delete(&h.app, &h.token, &format!("/crm/activities/{id}")).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["deleted"], true);
    // Deleting twice is a 404: the entry really is gone.
    let (status, _) = delete(&h.app, &h.token, &format!("/crm/activities/{id}")).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn an_entry_states_something_and_says_what_kind_of_thing_it_is() {
    let h = harness("crmact-422").await;
    let deal = a_deal(&h).await;

    for (bad, detail) in [
        (json!({}), "body must not be empty"),
        (json!({ "body": "   " }), "body must not be empty"),
        (
            json!({ "body": "x", "kind": "email" }),
            "kind must be one of note, call, meeting",
        ),
        (
            json!({ "body": "x", "happenedAt": "2026-08-07" }),
            "happenedAt must be an RFC 3339 timestamp",
        ),
    ] {
        let (status, body) = post(
            &h.app,
            &h.token,
            &format!("/crm/deals/{deal}/activities"),
            bad.clone(),
        )
        .await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{bad}: {body}");
        assert_eq!(body["detail"], detail, "{bad}");
    }

    // A body that is not JSON at all is a 400 that never echoes the input.
    let req = Request::builder()
        .method("POST")
        .uri(format!("/crm/deals/{deal}/activities"))
        .header("content-type", "application/json")
        .header("authorization", format!("Bearer {}", h.token))
        .body(Body::from("{not json"))
        .unwrap();
    let (status, body) = send(&h.app, req).await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    assert_eq!(body["detail"], "malformed request body");
}

#[tokio::test]
async fn a_deal_of_another_tenant_answers_as_one_that_never_existed() {
    let a = harness("crmact-a").await;
    let b = harness("crmact-b").await;

    let a_deal_id = a_deal(&a).await;
    let (status, body) = post(
        &a.app,
        &a.token,
        &format!("/crm/deals/{a_deal_id}/activities"),
        json!({ "body": "Ours." }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let entry = body["activity"]["id"].as_str().unwrap().to_owned();

    for uri in [
        format!("/crm/deals/{a_deal_id}/activities"),
        format!("/crm/deals/{a_deal_id}/next-steps"),
        "/crm/deals/crd_nope/activities".to_owned(),
        "/crm/deals/crd_nope/next-steps".to_owned(),
    ] {
        let (status, body) = get(&b.app, &b.token, &uri).await;
        assert_eq!(status, StatusCode::NOT_FOUND, "{uri}: {body}");
        let (status, body) =
            post(&b.app, &b.token, &uri, json!({ "body": "x", "title": "x" })).await;
        assert_eq!(status, StatusCode::NOT_FOUND, "{uri}: {body}");
    }
    for id in [entry.as_str(), "cra_nope"] {
        let (status, body) = delete(&b.app, &b.token, &format!("/crm/activities/{id}")).await;
        assert_eq!(status, StatusCode::NOT_FOUND, "{id}: {body}");
    }

    // A's record survived every attempt.
    let (_, body) = get(
        &a.app,
        &a.token,
        &format!("/crm/deals/{a_deal_id}/activities"),
    )
    .await;
    assert_eq!(body["activities"].as_array().map(Vec::len), Some(1));
}

// ---- next steps ---------------------------------------------------------------

#[tokio::test]
async fn a_next_step_is_a_task_the_tasks_module_also_shows() {
    let h = harness("crmact-step").await;
    let deal = a_deal(&h).await;

    let (status, body) = get(&h.app, &h.token, &format!("/crm/deals/{deal}/next-steps")).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["nextSteps"].as_array().map(Vec::len), Some(0));

    let (status, body) = post(
        &h.app,
        &h.token,
        &format!("/crm/deals/{deal}/next-steps"),
        json!({
            "title": "  Send the renewal quote  ",
            "dueAt": "2026-08-14T09:00:00Z",
            "priority": "high",
        }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let task = body["nextStep"].clone();
    let task_id = task["id"].as_str().unwrap().to_owned();
    assert_eq!(task["title"], "Send the renewal quote");
    assert_eq!(task["dueAt"], "2026-08-14T09:00:00Z");
    assert_eq!(task["priority"], "high");
    // The link is written by us, in ADR 0021's own vocabulary.
    assert_eq!(task["sourceKind"], "deal");
    assert_eq!(task["sourceId"], deal.as_str());
    assert_eq!(task["state"], "active");

    // It shows in the deal, with the date it is due…
    let (status, body) = get(&h.app, &h.token, &format!("/crm/deals/{deal}/next-steps")).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["nextSteps"].as_array().map(Vec::len), Some(1));
    assert_eq!(body["nextSteps"][0]["id"], task_id.as_str());
    assert_eq!(body["nextSteps"][0]["dueAt"], "2026-08-14T09:00:00Z");

    // …and it is a real task: the tasks module answers for the same row, which
    // is the whole point of not keeping a second to-do list.
    let (status, body) = get(&h.app, &h.token, &format!("/tasks/{task_id}")).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["task"]["sourceId"], deal.as_str());
    assert_eq!(body["task"]["title"], "Send the renewal quote");
}

#[tokio::test]
async fn a_next_step_states_a_title_and_a_real_due_date() {
    let h = harness("crmact-step422").await;
    let deal = a_deal(&h).await;

    for (bad, detail) in [
        (json!({}), "title is required"),
        (json!({ "title": "  " }), "title is required"),
        (
            json!({ "title": "Call Ada", "dueAt": "next tuesday" }),
            "dueAt must be an RFC 3339 timestamp",
        ),
    ] {
        let (status, body) = post(
            &h.app,
            &h.token,
            &format!("/crm/deals/{deal}/next-steps"),
            bad.clone(),
        )
        .await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{bad}: {body}");
        assert_eq!(body["detail"], detail, "{bad}");
    }

    // A project the caller cannot see is the same 404 a deal of another tenant
    // gets — the tasks module's own rule, answered in CRM's vocabulary.
    let (status, body) = post(
        &h.app,
        &h.token,
        &format!("/crm/deals/{deal}/next-steps"),
        json!({ "title": "Into somebody else's list", "projectId": "proj_nope" }),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND, "{body}");
}

// ---- the guards --------------------------------------------------------------

#[tokio::test]
async fn only_the_author_may_delete_an_entry() {
    let h = harness("crmact-403").await;
    let deal = a_deal(&h).await;
    let (status, body) = post(
        &h.app,
        &h.token,
        &format!("/crm/deals/{deal}/activities"),
        json!({ "body": "Called Ada, she is in." }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let entry = body["activity"]["id"].as_str().unwrap().to_owned();

    // A colleague of the same tenant: a second user, logged in for real.
    let email = format!("mate-{}@example.test", h.tenant);
    let user = h.ts.create_user(&email).await.unwrap();
    h.identity
        .set_password(&h.tenant, &user, &email, "s3cret-pw")
        .await
        .unwrap();
    let mate = h
        .identity
        .password_login(&email, "s3cret-pw", None)
        .await
        .unwrap()
        .expect("token issued")
        .0
        .reveal()
        .to_owned();

    // They read the log — it is tenant-wide, like the deal…
    let (status, body) = get(&h.app, &mate, &format!("/crm/deals/{deal}/activities")).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["activities"].as_array().map(Vec::len), Some(1));

    // …but the entry is not theirs to remove, and they are told exactly that
    // rather than that it does not exist: they are looking straight at it.
    let (status, body) = delete(&h.app, &mate, &format!("/crm/activities/{entry}")).await;
    assert_eq!(status, StatusCode::FORBIDDEN, "{body}");
    let (_, body) = get(&h.app, &h.token, &format!("/crm/deals/{deal}/activities")).await;
    assert_eq!(body["activities"].as_array().map(Vec::len), Some(1));
}

#[tokio::test]
async fn every_route_refuses_an_unauthenticated_caller() {
    let h = harness("crmact-401").await;
    let deal = a_deal(&h).await;

    let mut unauthenticated: Vec<Request<Body>> = vec![
        with_json(
            "POST",
            &format!("/crm/deals/{deal}/activities"),
            None,
            json!({ "body": "x" }),
        ),
        with_json(
            "POST",
            &format!("/crm/deals/{deal}/next-steps"),
            None,
            json!({ "title": "x" }),
        ),
    ];
    for uri in [
        format!("/crm/deals/{deal}/activities"),
        format!("/crm/deals/{deal}/next-steps"),
    ] {
        unauthenticated.push(Request::builder().uri(uri).body(Body::empty()).unwrap());
    }
    unauthenticated.push(
        Request::builder()
            .method("DELETE")
            .uri("/crm/activities/cra_1")
            .body(Body::empty())
            .unwrap(),
    );

    for req in unauthenticated {
        let (method, uri) = (req.method().clone(), req.uri().clone());
        let (status, body) = send(&h.app, req).await;
        assert_eq!(status, StatusCode::UNAUTHORIZED, "{method} {uri}: {body}");
    }
}
