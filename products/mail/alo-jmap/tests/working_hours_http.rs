//! `GET/PUT /calendar/working-hours` and the second span kind it gives
//! free/busy. The schedule round-trips through the wire spelling (ISO weekday
//! numbers, `"HH:MM"`, IANA zone or null), nonsense is refused verbatim at
//! 422, and `/calendar/freebusy` serves `outsideHours` beside `busy` without
//! changing what `busy` says.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use serde_json::{Value, json};

use crate::common::{get, harness, send};

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

#[tokio::test]
async fn the_default_schedule_is_served_before_anyone_sets_one() {
    let h = harness("wh-default").await;
    let (status, body) = get(&h.app, &h.token, "/calendar/working-hours").await;
    assert!(status.is_success(), "{status}");
    assert_eq!(body["days"], json!([1, 2, 3, 4, 5]), "{body}");
    assert_eq!(body["start"], json!("09:00"));
    assert_eq!(body["end"], json!("17:00"));
    assert_eq!(body["zone"], Value::Null);
}

#[tokio::test]
async fn a_schedule_round_trips_in_the_wire_spelling() {
    let h = harness("wh-rt").await;
    let (status, saved) = put(
        &h.app,
        &h.token,
        "/calendar/working-hours",
        json!({
            "days": [2, 3, 5],
            "start": "08:30",
            "end": "16:00",
            "zone": "Europe/Brussels",
        }),
    )
    .await;
    assert!(status.is_success(), "{status}: {saved}");
    let (status, body) = get(&h.app, &h.token, "/calendar/working-hours").await;
    assert!(status.is_success());
    assert_eq!(body["days"], json!([2, 3, 5]), "{body}");
    assert_eq!(body["start"], json!("08:30"));
    assert_eq!(body["end"], json!("16:00"));
    assert_eq!(body["zone"], json!("Europe/Brussels"));
}

#[tokio::test]
async fn nonsense_is_refused_verbatim_and_nothing_sticks() {
    let h = harness("wh-refuse").await;
    for (body, why) in [
        (
            json!({"days": [1, 8], "start": "09:00", "end": "17:00"}),
            "day 8",
        ),
        (
            json!({"days": [1], "start": "17:00", "end": "09:00"}),
            "backwards window",
        ),
        (
            json!({"days": [1], "start": "09:00", "end": "17:00", "zone": "Romance Standard Time"}),
            "a Windows zone name",
        ),
        (
            json!({"days": [1], "start": "quarter past", "end": "17:00"}),
            "not a clock time",
        ),
    ] {
        let (status, problem) = put(&h.app, &h.token, "/calendar/working-hours", body).await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{why}: {problem}");
        assert!(
            problem["detail"].is_string(),
            "{why} names itself: {problem}"
        );
    }
    let (_, body) = get(&h.app, &h.token, "/calendar/working-hours").await;
    assert_eq!(body["start"], json!("09:00"), "refusals left the default");
}

#[tokio::test]
async fn the_routes_require_a_bearer_token() {
    let h = harness("wh-auth").await;
    let (status, _) = get(&h.app, "not-a-token", "/calendar/working-hours").await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    let (status, _) = put(
        &h.app,
        "not-a-token",
        "/calendar/working-hours",
        json!({"days": [1], "start": "09:00", "end": "17:00"}),
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn freebusy_serves_outside_hours_beside_busy_untouched() {
    let h = harness("wh-fb").await;

    // One event Tue 2026-09-01 10:00–11:00Z on the caller's own calendar.
    let (status, _) = post(
        &h.app,
        &h.token,
        "/calendar/events",
        json!({
            "summary": "standup",
            "startsAt": "2026-09-01T10:00:00Z",
            "endsAt": "2026-09-01T11:00:00Z",
        }),
    )
    .await;
    assert!(status.is_success(), "{status}");

    let (status, body) = post(
        &h.app,
        &h.token,
        "/calendar/freebusy",
        json!({
            "emails": [h.email],
            "from": "2026-09-01T00:00:00Z",
            "to": "2026-09-02T00:00:00Z",
        }),
    )
    .await;
    assert!(status.is_success(), "{status}: {body}");
    let person = &body["freebusy"][0];
    assert_eq!(person["known"], json!(true), "{body}");
    // Busy is exactly what it always was: the event, nothing else.
    assert_eq!(
        person["busy"],
        json!([{"start": "2026-09-01T10:00:00Z", "end": "2026-09-01T11:00:00Z"}]),
        "{body}"
    );
    // The second kind: the default Mon–Fri 09:00–17:00 (no zone set → UTC)
    // leaves the night and the evening of that Tuesday outside.
    assert_eq!(
        person["outsideHours"],
        json!([
            {"start": "2026-09-01T00:00:00Z", "end": "2026-09-01T09:00:00Z"},
            {"start": "2026-09-01T17:00:00Z", "end": "2026-09-02T00:00:00Z"},
        ]),
        "{body}"
    );

    // A schedule with a zone moves the spans; busy stays put.
    let (status, _) = put(
        &h.app,
        &h.token,
        "/calendar/working-hours",
        json!({"days": [1, 2, 3, 4, 5], "start": "09:00", "end": "17:00", "zone": "Europe/Brussels"}),
    )
    .await;
    assert!(status.is_success());
    let (_, body) = post(
        &h.app,
        &h.token,
        "/calendar/freebusy",
        json!({
            "emails": [h.email],
            "from": "2026-09-01T00:00:00Z",
            "to": "2026-09-02T00:00:00Z",
        }),
    )
    .await;
    let person = &body["freebusy"][0];
    // CEST (UTC+2): 09:00–17:00 local is 07:00–15:00Z.
    assert_eq!(
        person["outsideHours"][0]["end"],
        json!("2026-09-01T07:00:00Z"),
        "{body}"
    );
    assert_eq!(
        person["busy"],
        json!([{"start": "2026-09-01T10:00:00Z", "end": "2026-09-01T11:00:00Z"}]),
        "busy never moves with the schedule: {body}"
    );

    // A stranger's email stays unknown, with both kinds empty.
    let (_, body) = post(
        &h.app,
        &h.token,
        "/calendar/freebusy",
        json!({
            "emails": ["nobody@example.test"],
            "from": "2026-09-01T00:00:00Z",
            "to": "2026-09-02T00:00:00Z",
        }),
    )
    .await;
    let person = &body["freebusy"][0];
    assert_eq!(person["known"], json!(false), "{body}");
    assert_eq!(person["outsideHours"], json!([]), "{body}");
}
