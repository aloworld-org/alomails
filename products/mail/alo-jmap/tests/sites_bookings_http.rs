//! The `/sites/{id}/bookings*` edit surface (ADR 0036, S2.13a), driven through
//! the real router over a real Postgres.
//!
//! `alo-store`'s suite proves the storage; what this suite pins is the **edge**:
//! the auth guard, the Agenda calendar named by id and resolved server-side,
//! the week and the questions travelling in camelCase, the defaults a first
//! service can be created with — and, mandatory, that another tenant's booking
//! service and another tenant's calendar are invisible on every verb.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::sync::Arc;

use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use serde_json::{Value, json};

use crate::common::{Harness, harness, harness_on, send};

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
    let req = Request::builder()
        .method("GET")
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

/// A subdomain unique to this harness run — the global namespace is shared.
fn sub(tag: &str, h: &Harness) -> String {
    let salt: String = h
        .tenant
        .as_str()
        .chars()
        .filter(char::is_ascii_alphanumeric)
        .map(|c| c.to_ascii_lowercase())
        .take(20)
        .collect();
    format!("{tag}{salt}")
}

async fn site_of(h: &Harness, tag: &str) -> String {
    let (status, body) = post(
        &h.app,
        &h.token,
        "/sites",
        json!({ "name": "Studio", "subdomain": sub(tag, h) }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "create site failed: {body}");
    body["id"].as_str().expect("created site").to_owned()
}

/// The account's own calendar, through Agenda's own door — a booking service
/// never invents one.
async fn calendar_of(h: &Harness) -> String {
    h.acc
        .ensure_personal_calendar()
        .await
        .unwrap()
        .as_str()
        .to_owned()
}

fn body_for(calendar: &str) -> Value {
    json!({
        "name": "Consultation",
        "description": "Half an hour, in the studio.",
        "calendarId": calendar,
        "timeZone": "Europe/Brussels",
        "durationMinutes": 30,
        "hours": [
            { "weekday": 1, "startMinute": 540, "endMinute": 720 },
            { "weekday": 3, "startMinute": 540, "endMinute": 720 },
        ],
    })
}

#[tokio::test]
async fn booking_services_are_created_read_replaced_and_deleted_through_the_account_door() {
    let owner = harness("sites-bookings-http").await;
    let site = site_of(&owner, "book").await;
    let calendar = calendar_of(&owner).await;

    // The picker offers the account's own calendars, writable.
    let (status, sources) = get(
        &owner.app,
        &owner.token,
        &format!("/sites/{site}/booking-sources"),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{sources}");
    let listed = sources["sources"].as_array().unwrap();
    let personal = listed
        .iter()
        .find(|source| source["id"] == calendar.as_str())
        .expect("the account's own calendar");
    assert_eq!(personal["writable"], true);

    let (status, empty) = get(&owner.app, &owner.token, &format!("/sites/{site}/bookings")).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(empty["bookings"].as_array().unwrap().len(), 0);

    let (status, created) = post(
        &owner.app,
        &owner.token,
        &format!("/sites/{site}/bookings"),
        body_for(&calendar),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{created}");
    let booking = created["id"].as_str().unwrap().to_owned();
    // What was not sent carries its default: two months of horizon, no buffer,
    // no notice, no extra questions, and a service that is on.
    assert_eq!(created["horizonDays"], 60);
    assert_eq!(created["bufferMinutes"], 0);
    assert_eq!(created["noticeMinutes"], 0);
    assert_eq!(created["active"], true);
    assert_eq!(created["fields"].as_array().unwrap().len(), 0);
    // The calendar is named by id and described by the server.
    assert_eq!(created["calendarId"], calendar.as_str());
    assert_eq!(created["calendar"]["id"], calendar.as_str());
    assert_eq!(created["calendar"]["writable"], true);
    assert_eq!(created["hours"][0]["weekday"], 1);
    assert_eq!(created["hours"][0]["startMinute"], 540);
    assert_eq!(created["hours"][0]["endMinute"], 720);

    let (status, listed) = get(&owner.app, &owner.token, &format!("/sites/{site}/bookings")).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(listed["bookings"].as_array().unwrap().len(), 1);
    assert_eq!(listed["bookings"][0]["name"], "Consultation");

    // A replace is whole, and the questions arrive with it.
    let mut replacement = body_for(&calendar);
    replacement["name"] = json!("Long consultation");
    replacement["durationMinutes"] = json!(60);
    replacement["bufferMinutes"] = json!(15);
    replacement["noticeMinutes"] = json!(240);
    replacement["horizonDays"] = json!(30);
    replacement["active"] = json!(false);
    replacement["fields"] = json!([
        { "key": "phone", "label": "Phone number", "kind": "phone", "required": true },
        {
            "key": "treatment",
            "label": "Which treatment?",
            "kind": "choice",
            "options": ["Cut", "Colour"],
        },
    ]);
    let (status, replaced) = put(
        &owner.app,
        &owner.token,
        &format!("/sites/{site}/bookings/{booking}"),
        replacement,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{replaced}");
    assert_eq!(replaced["name"], "Long consultation");
    assert_eq!(replaced["durationMinutes"], 60);
    assert_eq!(replaced["active"], false);
    assert_eq!(replaced["fields"][0]["kind"], "phone");
    assert_eq!(replaced["fields"][0]["required"], true);
    assert_eq!(replaced["fields"][1]["options"][1], "Colour");

    let (status, read) = get(
        &owner.app,
        &owner.token,
        &format!("/sites/{site}/bookings/{booking}"),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(read["fields"].as_array().unwrap().len(), 2);

    let (status, _) = delete(
        &owner.app,
        &owner.token,
        &format!("/sites/{site}/bookings/{booking}"),
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    let (status, gone) = get(
        &owner.app,
        &owner.token,
        &format!("/sites/{site}/bookings/{booking}"),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND, "{gone}");
}

#[tokio::test]
async fn the_edge_answers_a_broken_request_with_the_reason_and_stores_nothing() {
    let owner = harness("sites-bookings-refusals").await;
    let site = site_of(&owner, "refuse").await;
    let calendar = calendar_of(&owner).await;

    // Unauthenticated, on both a read and a write.
    let anonymous = Request::builder()
        .method("GET")
        .uri(format!("/sites/{site}/bookings"))
        .body(Body::empty())
        .unwrap();
    let (status, _) = send(&owner.app, anonymous).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    let (status, _) = send(
        &owner.app,
        with_json(
            "POST",
            &format!("/sites/{site}/bookings"),
            None,
            body_for(&calendar),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);

    // A body that is not the shape is a 400, before any rule is consulted.
    let (status, _) = post(
        &owner.app,
        &owner.token,
        &format!("/sites/{site}/bookings"),
        json!({ "name": "Consultation", "surprise": true }),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);

    // A rule the store names comes back verbatim, as a 422.
    let mut zone = body_for(&calendar);
    zone["timeZone"] = json!("Middle/Earth");
    let (status, said) = post(
        &owner.app,
        &owner.token,
        &format!("/sites/{site}/bookings"),
        zone,
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert!(
        said["detail"]
            .as_str()
            .unwrap_or_default()
            .contains("Europe/Brussels"),
        "{said}"
    );

    let mut kind = body_for(&calendar);
    kind["fields"] = json!([{ "key": "phone", "label": "Phone", "kind": "dropdown" }]);
    let (status, said) = post(
        &owner.app,
        &owner.token,
        &format!("/sites/{site}/bookings"),
        kind,
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert!(
        said["detail"]
            .as_str()
            .unwrap_or_default()
            .contains("long_text"),
        "{said}"
    );

    // A calendar nobody owns is a 404, exactly like a site of another tenant.
    let mut nobody = body_for(&calendar);
    nobody["calendarId"] = json!("cal-nobody");
    let (status, _) = post(
        &owner.app,
        &owner.token,
        &format!("/sites/{site}/bookings"),
        nobody,
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    let (status, still_empty) =
        get(&owner.app, &owner.token, &format!("/sites/{site}/bookings")).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(still_empty["bookings"].as_array().unwrap().len(), 0);
}

#[tokio::test]
async fn another_tenants_booking_service_and_calendar_are_invisible_on_every_verb() {
    let owner = harness("sites-bookings-owner").await;
    let site = site_of(&owner, "own").await;
    let calendar = calendar_of(&owner).await;
    let booking = post(
        &owner.app,
        &owner.token,
        &format!("/sites/{site}/bookings"),
        body_for(&calendar),
    )
    .await
    .1["id"]
        .as_str()
        .unwrap()
        .to_owned();

    let outsider = harness_on(Arc::clone(&owner.store), "sites-bookings-outsider").await;
    let outsider_site = site_of(&outsider, "out").await;

    // The owner's site is not the outsider's, on every route.
    for uri in [
        format!("/sites/{site}/bookings"),
        format!("/sites/{site}/booking-sources"),
        format!("/sites/{site}/bookings/{booking}"),
    ] {
        let (status, body) = get(&outsider.app, &outsider.token, &uri).await;
        assert_eq!(status, StatusCode::NOT_FOUND, "{uri} leaked: {body}");
    }
    let (status, _) = put(
        &outsider.app,
        &outsider.token,
        &format!("/sites/{site}/bookings/{booking}"),
        body_for(&calendar_of(&outsider).await),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let (status, _) = delete(
        &outsider.app,
        &outsider.token,
        &format!("/sites/{site}/bookings/{booking}"),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    // Nor is the owner's calendar a source the outsider can bind on its own
    // site: it does not resolve, so it is a 404 and not a refusal that would
    // confirm the id exists somewhere.
    let (status, sources) = get(
        &outsider.app,
        &outsider.token,
        &format!("/sites/{outsider_site}/booking-sources"),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert!(
        !sources["sources"]
            .as_array()
            .unwrap()
            .iter()
            .any(|source| source["id"] == calendar.as_str()),
        "{sources}"
    );
    let (status, _) = post(
        &outsider.app,
        &outsider.token,
        &format!("/sites/{outsider_site}/bookings"),
        body_for(&calendar),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    // The owner's service is untouched by all of it.
    let (status, intact) = get(
        &owner.app,
        &owner.token,
        &format!("/sites/{site}/bookings/{booking}"),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(intact["name"], "Consultation");
    assert_eq!(intact["calendarId"], calendar.as_str());
}
