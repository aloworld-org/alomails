#![allow(clippy::unwrap_used)]
use crate::common::harness;
use axum::body::Body;
use axum::http::{Method, Request, StatusCode};
use serde_json::{Value, json};
use tower::ServiceExt;

async fn call(
    app: &axum::Router,
    token: &str,
    method: Method,
    uri: &str,
    body: Value,
) -> (StatusCode, Value) {
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method(method)
                .uri(uri)
                .header("authorization", format!("Bearer {token}"))
                .header("content-type", "application/json")
                .body(Body::from(body.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), 1024 * 1024)
        .await
        .unwrap();
    let value = if bytes.is_empty() {
        Value::Null
    } else {
        serde_json::from_slice(&bytes).unwrap()
    };
    (status, value)
}

#[tokio::test]
async fn a_schedule_is_persisted_tenant_scoped_and_deletable() {
    let h = harness("finschedule").await;
    h.ts.set_admin(&h.user, true).await.unwrap();
    let (status, body) = call(
        &h.app,
        &h.token,
        Method::POST,
        "/finance/report-schedules",
        json!({"report":"pl","cadence":"monthly","recipient":h.email,"nextRunDate":"2026-10-01"}),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let id = body["schedule"]["id"].as_str().unwrap();
    let (status, list) = call(
        &h.app,
        &h.token,
        Method::GET,
        "/finance/report-schedules",
        Value::Null,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(list["schedules"][0]["nextRunDate"], "2026-10-01");
    let (status, _) = call(
        &h.app,
        &h.token,
        Method::DELETE,
        &format!("/finance/report-schedules/{id}"),
        Value::Null,
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT);
}

#[tokio::test]
async fn a_schedule_cannot_deliver_outside_the_workspace() {
    let h = harness("finscheduleoutside").await;
    h.ts.set_admin(&h.user, true).await.unwrap();
    let(status,_)=call(&h.app,&h.token,Method::POST,"/finance/report-schedules",json!({"report":"vat","cadence":"weekly","recipient":"outsider@example.test","nextRunDate":"2026-10-01"})).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
}

#[tokio::test]
async fn a_due_schedule_generates_delivers_and_advances() {
    let h = harness("finschedulerun").await;
    h.ts.set_admin(&h.user, true).await.unwrap();
    let today = time::OffsetDateTime::now_utc().date().to_string();
    let (status, _) = call(
        &h.app,
        &h.token,
        Method::POST,
        "/finance/report-schedules",
        json!({"report":"pl","cadence":"weekly","recipient":h.email,"nextRunDate":today}),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(alo_jmap::finance_report_worker::run_due(&h.store).await, 1);
    let (_, list) = call(
        &h.app,
        &h.token,
        Method::GET,
        "/finance/report-schedules",
        Value::Null,
    )
    .await;
    assert!(!list["schedules"][0]["lastRunAt"].is_null());
    assert_ne!(list["schedules"][0]["nextRunDate"], today);
}
