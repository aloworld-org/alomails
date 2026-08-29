//! Rooms and resources on the wire: the admin CRUD, booking a room by naming
//! it on a meeting, the double-booking refusal, and the free/busy answer a
//! room gives in the same currency a person gives.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use serde_json::{Value, json};

use crate::common::{Harness, harness, send};

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

/// A harness whose user runs the workspace, plus one room.
async fn with_room(tag: &str, email: &str) -> (Harness, String) {
    let h = harness(tag).await;
    h.ts.set_admin(&h.user, true).await.unwrap();
    let (status, created) = post(
        &h.app,
        &h.token,
        "/calendar/resources",
        json!({"name": "Board room", "email": email, "location": "2nd floor", "capacity": 8}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{created}");
    let id = created["id"].as_str().unwrap().to_owned();
    (h, id)
}

#[tokio::test]
async fn a_room_is_created_listed_edited_and_retired_by_an_admin() {
    let (h, id) = with_room("res-crud", "board-crud@example.test").await;

    let (status, listed) = get(&h.app, &h.token, "/calendar/resources").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(listed["resources"].as_array().unwrap().len(), 1);
    assert_eq!(listed["resources"][0]["name"], "Board room");
    assert_eq!(listed["resources"][0]["capacity"], 8);
    assert_eq!(listed["resources"][0]["location"], "2nd floor");

    let (status, updated) = put(
        &h.app,
        &h.token,
        &format!("/calendar/resources/{id}"),
        json!({"name": "Board room (big)", "email": "board-crud@example.test", "capacity": 12}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{updated}");
    assert_eq!(updated["name"], "Board room (big)");
    assert_eq!(updated["location"], Value::Null);

    let (status, _) = delete(&h.app, &h.token, &format!("/calendar/resources/{id}")).await;
    assert_eq!(status, StatusCode::OK);
    let (_, listed) = get(&h.app, &h.token, "/calendar/resources").await;
    assert!(listed["resources"].as_array().unwrap().is_empty());
    // Retiring the same room twice is a plain 404, not a 500.
    let (status, _) = delete(&h.app, &h.token, &format!("/calendar/resources/{id}")).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn only_an_admin_may_shape_the_room_list_and_only_a_caller_may_read_it() {
    let h = harness("res-gate").await;
    // Not an admin: reading the rooms is fine, changing them is not.
    let (status, _) = get(&h.app, &h.token, "/calendar/resources").await;
    assert_eq!(status, StatusCode::OK);
    for (method, uri) in [
        ("POST", "/calendar/resources".to_owned()),
        ("PUT", "/calendar/resources/whatever".to_owned()),
    ] {
        let body = json!({"name": "Mine", "email": "mine@example.test"});
        let (status, problem) = if method == "POST" {
            post(&h.app, &h.token, &uri, body).await
        } else {
            put(&h.app, &h.token, &uri, body).await
        };
        assert_eq!(status, StatusCode::FORBIDDEN, "{method} {uri}: {problem}");
    }
    let (status, _) = delete(&h.app, &h.token, "/calendar/resources/whatever").await;
    assert_eq!(status, StatusCode::FORBIDDEN);

    // And nothing at all without a token.
    for uri in ["/calendar/resources"] {
        let req = Request::builder().uri(uri).body(Body::empty()).unwrap();
        let (status, _) = send(&h.app, req).await;
        assert_eq!(status, StatusCode::UNAUTHORIZED, "{uri}");
    }
}

#[tokio::test]
async fn a_nonsense_room_is_refused_verbatim() {
    let h = harness("res-bad").await;
    h.ts.set_admin(&h.user, true).await.unwrap();

    let (status, problem) = post(
        &h.app,
        &h.token,
        "/calendar/resources",
        json!({"name": "", "email": "board-bad@example.test"}),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{problem}");
    assert!(
        problem["detail"]
            .as_str()
            .unwrap()
            .contains("name of 1 to 120 characters"),
        "{problem}"
    );

    let (status, problem) = post(
        &h.app,
        &h.token,
        "/calendar/resources",
        json!({"name": "Board room", "email": "board"}),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{problem}");

    // A person's address is a conflict, not a validation error: it is
    // well-formed, it is simply already somebody.
    let (status, problem) = post(
        &h.app,
        &h.token,
        "/calendar/resources",
        json!({"name": "Me", "email": h.email}),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT, "{problem}");
}

#[tokio::test]
async fn naming_a_room_on_a_meeting_books_it_and_a_second_meeting_is_refused() {
    let (h, id) = with_room("res-book", "board-book@example.test").await;

    let (status, first) = post(
        &h.app,
        &h.token,
        "/calendar/events",
        json!({
            "summary": "Board meeting",
            "startsAt": "2026-09-02T10:00:00Z",
            "endsAt": "2026-09-02T11:00:00Z",
            "attendees": ["board-book@example.test"],
        }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{first}");
    let first_id = first["id"].as_str().unwrap().to_owned();

    // The room's own schedule now holds it.
    let held = h
        .acc
        .resource_bookings_in_range(
            &alo_store::CalendarId::new(id.clone()),
            time::OffsetDateTime::parse(
                "2026-09-02T00:00:00Z",
                &time::format_description::well_known::Rfc3339,
            )
            .unwrap(),
            time::OffsetDateTime::parse(
                "2026-09-03T00:00:00Z",
                &time::format_description::well_known::Rfc3339,
            )
            .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(held.len(), 1);

    // A second meeting over the same hour is refused, and nothing is written.
    let (status, problem) = post(
        &h.app,
        &h.token,
        "/calendar/events",
        json!({
            "summary": "Other meeting",
            "startsAt": "2026-09-02T10:30:00Z",
            "endsAt": "2026-09-02T11:30:00Z",
            "attendees": ["board-book@example.test"],
        }),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT, "{problem}");
    assert!(
        problem["detail"].as_str().unwrap().contains("Board room"),
        "{problem}"
    );
    let (_, listed) = get(
        &h.app,
        &h.token,
        "/calendar/events?from=2026-09-02T00:00:00Z&to=2026-09-03T00:00:00Z",
    )
    .await;
    assert_eq!(listed["events"].as_array().unwrap().len(), 1, "{listed}");

    // Moving the first meeting to an hour the room is free succeeds…
    let (status, moved) = put(
        &h.app,
        &h.token,
        &format!("/calendar/events/{first_id}"),
        json!({
            "summary": "Board meeting",
            "startsAt": "2026-09-02T14:00:00Z",
            "endsAt": "2026-09-02T15:00:00Z",
            "attendees": ["board-book@example.test"],
        }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{moved}");
    // …and the 10:00 slot is now bookable.
    let (status, second) = post(
        &h.app,
        &h.token,
        "/calendar/events",
        json!({
            "summary": "Other meeting",
            "startsAt": "2026-09-02T10:30:00Z",
            "endsAt": "2026-09-02T11:30:00Z",
            "attendees": ["board-book@example.test"],
        }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{second}");

    // Dropping the room from the guest list releases it.
    let (status, _) = put(
        &h.app,
        &h.token,
        &format!("/calendar/events/{first_id}"),
        json!({
            "summary": "Board meeting",
            "startsAt": "2026-09-02T14:00:00Z",
            "endsAt": "2026-09-02T15:00:00Z",
            "attendees": [],
        }),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let (status, third) = post(
        &h.app,
        &h.token,
        "/calendar/events",
        json!({
            "summary": "Third meeting",
            "startsAt": "2026-09-02T14:00:00Z",
            "endsAt": "2026-09-02T15:00:00Z",
            "attendees": ["board-book@example.test"],
        }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{third}");
}

#[tokio::test]
async fn free_busy_answers_for_a_room_the_way_it_answers_for_a_person() {
    let (h, _) = with_room("res-fb", "board-fb@example.test").await;

    let (status, made) = post(
        &h.app,
        &h.token,
        "/calendar/events",
        json!({
            "summary": "Board meeting",
            "startsAt": "2026-09-02T10:00:00Z",
            "endsAt": "2026-09-02T11:00:00Z",
            "attendees": ["board-fb@example.test"],
        }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{made}");

    let (status, fb) = post(
        &h.app,
        &h.token,
        "/calendar/freebusy",
        json!({
            "from": "2026-09-02T00:00:00Z",
            "to": "2026-09-03T00:00:00Z",
            "emails": ["board-fb@example.test", "nobody@example.test"],
        }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{fb}");
    let room = &fb["freebusy"][0];
    assert_eq!(room["known"], true, "{fb}");
    assert_eq!(room["kind"], "resource", "{fb}");
    assert_eq!(room["busy"][0]["start"], "2026-09-02T10:00:00Z", "{fb}");
    assert_eq!(room["busy"][0]["end"], "2026-09-02T11:00:00Z", "{fb}");
    // A room keeps no hours: it is free whenever nobody has it.
    assert!(room["outsideHours"].as_array().unwrap().is_empty(), "{fb}");
    // A stranger is still a stranger.
    assert_eq!(fb["freebusy"][1]["known"], false, "{fb}");
    assert_eq!(fb["freebusy"][1]["kind"], "unknown", "{fb}");
}

#[tokio::test]
async fn a_recurring_meeting_holds_its_room_at_every_occurrence() {
    let (h, _) = with_room("res-series", "board-series@example.test").await;

    let (status, made) = post(
        &h.app,
        &h.token,
        "/calendar/events",
        json!({
            "summary": "Weekly standup",
            "startsAt": "2026-09-02T10:00:00Z",
            "endsAt": "2026-09-02T11:00:00Z",
            "recurrence": "FREQ=WEEKLY;COUNT=4",
            "attendees": ["board-series@example.test"],
        }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{made}");

    // Three weeks out, an occurrence nobody typed still holds the room.
    let (status, problem) = post(
        &h.app,
        &h.token,
        "/calendar/events",
        json!({
            "summary": "Clash",
            "startsAt": "2026-09-23T10:30:00Z",
            "endsAt": "2026-09-23T11:30:00Z",
            "attendees": ["board-series@example.test"],
        }),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT, "{problem}");

    // Past the series' COUNT, the room is free again.
    let (status, after) = post(
        &h.app,
        &h.token,
        "/calendar/events",
        json!({
            "summary": "After the series",
            "startsAt": "2026-09-30T10:00:00Z",
            "endsAt": "2026-09-30T11:00:00Z",
            "attendees": ["board-series@example.test"],
        }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{after}");
}
