//! The Agenda JSON API's recurrence timezone (M3.2): an event created with a
//! `timezone` follows that zone's wall-clock across a DST switch in the range
//! listing (the same one expansion function CalDAV uses), the zone round-trips
//! through create → get, and an unknown zone is refused verbatim at write time
//! rather than stored and silently expanded as UTC.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use serde_json::{Value, json};

use crate::common::{harness, send};

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

#[tokio::test]
async fn recurring_event_timezone_round_trips_and_drives_expansion() {
    let h = harness("cal-tz").await;

    // Weekly 09:00 Brussels from Mon 2026-10-19 (CEST, UTC+2 → 07:00Z),
    // crossing the 2026-10-25 DST end.
    let (status, created) = post(
        &h.app,
        &h.token,
        "/calendar/events",
        json!({
            "summary": "Monday review",
            "startsAt": "2026-10-19T07:00:00Z",
            "endsAt": "2026-10-19T07:30:00Z",
            "recurrence": "FREQ=WEEKLY;COUNT=3",
            "timezone": "Europe/Brussels",
        }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{created}");
    assert_eq!(created["timezone"], "Europe/Brussels", "{created}");
    let id = created["id"].as_str().unwrap().to_owned();

    // The range listing answers 09:00 local on both sides of the switch:
    // 07:00Z while CEST holds, 08:00Z once CET returns.
    let (status, listed) = get(
        &h.app,
        &h.token,
        "/calendar/events?from=2026-10-15T00:00:00Z&to=2026-11-05T00:00:00Z",
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{listed}");
    let starts: Vec<&str> = listed["events"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|e| e["id"] == id.as_str())
        .map(|e| e["startsAt"].as_str().unwrap())
        .collect();
    assert_eq!(
        starts,
        vec![
            "2026-10-19T07:00:00Z",
            "2026-10-26T08:00:00Z",
            "2026-11-02T08:00:00Z",
        ],
        "{listed}"
    );

    // The series template the editor loads carries the zone.
    let (status, one) = get(&h.app, &h.token, &format!("/calendar/events/{id}")).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(one["timezone"], "Europe/Brussels", "{one}");

    // An unknown zone is refused with the name echoed back, and nothing stored.
    let (status, problem) = post(
        &h.app,
        &h.token,
        "/calendar/events",
        json!({
            "summary": "Wrong zone",
            "startsAt": "2026-10-19T07:00:00Z",
            "endsAt": "2026-10-19T07:30:00Z",
            "recurrence": "FREQ=WEEKLY",
            "timezone": "Romance Standard Time",
        }),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{problem}");
    assert!(
        problem["detail"]
            .as_str()
            .unwrap()
            .contains("Romance Standard Time"),
        "the refused zone is named verbatim: {problem}"
    );
}
