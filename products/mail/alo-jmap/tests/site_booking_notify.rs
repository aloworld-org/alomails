//! The booking-notification sweep (ADR 0036, S2.13b2), driven end-to-end over
//! a real Postgres and the real public router: a visitor takes an appointment
//! on a published website, and the site owner is told about it by ONE
//! internally delivered message in their own inbox — the right tenant's inbox
//! and nobody else's — exactly once.
//!
//! The arc is deliberately whole: the owner builds the service through the
//! authenticated `/sites/*` surface, the visitor books through `alo-sites`
//! with no credentials at all, and only then does the sweep run.

#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use std::sync::Arc;

use alo_sites::serve::{AppState as PublicAppState, app as public_app};
use alo_store::{BlobStore, Page, SitePublicStore};
use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode, header};
use base64::Engine;
use base64::engine::general_purpose::STANDARD as B64;
use serde_json::{Value, json};
use sqlx::postgres::PgPoolOptions;
use time::{Duration, OffsetDateTime};
use tower::ServiceExt;

use common::{Harness, database_url, harness, harness_on, send};

async fn post(app: &Router, token: &str, path: &str, body: Value) -> (StatusCode, Value) {
    let request = Request::builder()
        .method("POST")
        .uri(path)
        .header("authorization", format!("Bearer {token}"))
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(body.to_string()))
        .unwrap();
    send(app, request).await
}

fn created_id(kind: &str, (status, body): (StatusCode, Value)) -> String {
    assert_eq!(status, StatusCode::OK, "create {kind} failed: {body}");
    body["id"].as_str().expect("created id").to_owned()
}

/// A subdomain unique to this harness run — the namespace is global and the
/// compose Postgres is shared across runs.
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

/// Splits a raw RFC 5322 message into (headers, base64-decoded body text).
fn open_message(raw: &[u8]) -> (String, String) {
    let raw = String::from_utf8_lossy(raw).into_owned();
    let (headers, body) = raw.split_once("\r\n\r\n").expect("header/body split");
    let body_bytes = B64
        .decode(body.replace(['\r', '\n'], ""))
        .expect("base64 body");
    (
        headers.to_owned(),
        String::from_utf8(body_bytes).expect("utf-8 body"),
    )
}

/// The day the visitor is sent to: far enough ahead that no notice period and
/// no passing hour can make it empty, near enough to sit inside the horizon.
fn booking_day() -> String {
    let day = (OffsetDateTime::now_utc() + Duration::days(3)).date();
    format!(
        "{:04}-{:02}-{:02}",
        day.year(),
        u8::from(day.month()),
        day.day()
    )
}

/// One published, bookable service on `owner`'s site, built through the
/// authenticated editor surface exactly as the owner would. Open every day of
/// the week, in UTC, so the day above always offers something.
async fn published_service(owner: &Harness, tag: &str, service_name: &str) -> String {
    let site = created_id(
        "site",
        post(
            &owner.app,
            &owner.token,
            "/sites",
            json!({ "name": "Harbour Studio", "subdomain": sub(tag, owner) }),
        )
        .await,
    );
    let page = created_id(
        "page",
        post(
            &owner.app,
            &owner.token,
            &format!("/sites/{site}/pages"),
            json!({ "title": "Home", "home": true }),
        )
        .await,
    );
    let calendar = owner
        .acc
        .ensure_personal_calendar()
        .await
        .unwrap()
        .as_str()
        .to_owned();
    let booking = created_id(
        "booking",
        post(
            &owner.app,
            &owner.token,
            &format!("/sites/{site}/bookings"),
            json!({
                "name": service_name,
                "description": "Half an hour, in the studio.",
                "calendarId": calendar,
                "timeZone": "UTC",
                "durationMinutes": 30,
                "location": "Second floor",
                "hours": (1..=7)
                    .map(|weekday| json!({
                        "weekday": weekday,
                        "startMinute": 540,
                        "endMinute": 720,
                    }))
                    .collect::<Vec<_>>(),
                "fields": [
                    { "key": "phone", "label": "Phone", "kind": "phone", "required": true },
                ],
            }),
        )
        .await,
    );
    let (status, body) = post(
        &owner.app,
        &owner.token,
        &format!("/sites/{site}/pages/{page}/sections"),
        json!({ "section": {
            "type": "booking",
            "booking_id": booking,
            "heading": "Come and talk to us"
        }}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "add booking section failed: {body}");
    let (status, body) = post(
        &owner.app,
        &owner.token,
        &format!("/sites/{site}/publish"),
        json!({}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "publish failed: {body}");
    booking
}

/// The first free time the published page actually offers on [`booking_day`],
/// read out of the rendered radio buttons — the visitor's own view of the day.
async fn first_offered_slot(public: &Arc<PublicAppState>, booking: &str) -> String {
    let request = Request::builder()
        .method("GET")
        .uri(format!("/b/{booking}?date={}", booking_day()))
        .header(header::HOST, "sites.test")
        .body(Body::empty())
        .unwrap();
    let response = public_app(Arc::clone(public))
        .oneshot(request)
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK, "the day page is served");
    let html = String::from_utf8_lossy(
        &axum::body::to_bytes(response.into_body(), 1024 * 1024)
            .await
            .unwrap(),
    )
    .into_owned();
    let marker = "name=\"slot\" value=\"";
    let start = html.find(marker).expect("the day offers at least one time") + marker.len();
    html[start..]
        .split('"')
        .next()
        .expect("a slot value")
        .to_owned()
}

/// Takes `slot` as an anonymous visitor would, through the public router.
async fn book(
    public: &Arc<PublicAppState>,
    booking: &str,
    slot: &str,
    name: &str,
    email: &str,
    phone: &str,
) -> StatusCode {
    let form = format!(
        "slot={}&name={}&email={}&q-phone={}&website=",
        urlencode(slot),
        urlencode(name),
        urlencode(email),
        urlencode(phone)
    );
    let request = Request::builder()
        .method("POST")
        .uri(format!("/b/{booking}"))
        .header(header::HOST, "sites.test")
        .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
        .header("x-forwarded-for", "203.0.113.44")
        .body(Body::from(form))
        .unwrap();
    public_app(Arc::clone(public))
        .oneshot(request)
        .await
        .unwrap()
        .status()
}

/// Percent-encoding for the few characters these fixtures actually carry.
fn urlencode(value: &str) -> String {
    value
        .bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                (b as char).to_string()
            }
            b' ' => "+".to_owned(),
            other => format!("%{other:02X}"),
        })
        .collect()
}

/// One test on purpose: the sweep is global, so concurrently running scenarios
/// could claim (and deliver) each other's rows mid-assertion. Sequenced here,
/// every step's outcome is deterministic. (`.config/nextest.toml` keeps this
/// binary and `alo-store`'s `site_bookings_public` out of each other's way for
/// the same reason.)
#[tokio::test]
async fn a_booking_notifies_its_own_owner_once_and_nobody_else() {
    let a = harness("bknota").await;
    // Tenant B lives on the SAME store handle: production runs one process for
    // every tenant, and the sweep must serve them all.
    let b = harness_on(a.store.clone(), "bknotb").await;

    let booking_a = published_service(&a, "bka", "Consultation").await;
    let booking_b = published_service(&b, "bkb", "Beta-only fitting").await;

    let pool = PgPoolOptions::new()
        .max_connections(4)
        .connect(&database_url())
        .await
        .unwrap();
    let public = PublicAppState::new(
        SitePublicStore::new(pool, BlobStore::in_memory(1024 * 1024)),
        "sites.test".to_owned(),
        b"site-booking-notify-local-secret",
    );

    let slot_a = first_offered_slot(&public, &booking_a).await;
    assert_eq!(
        book(
            &public,
            &booking_a,
            &slot_a,
            "Ada Lovelace",
            "ada@example.test",
            "+32 2 555 01"
        )
        .await,
        StatusCode::OK,
        "the visitor's booking is taken"
    );
    let slot_b = first_offered_slot(&public, &booking_b).await;
    assert_eq!(
        book(
            &public,
            &booking_b,
            &slot_b,
            "Grace Hopper",
            "grace@example.test",
            "+1 555 0100"
        )
        .await,
        StatusCode::OK
    );

    // Before the sweep, the appointment is already in the owner's calendar —
    // which is why a lost notification loses nothing.
    let inbox_a = a.acc.inbox().await.unwrap();
    let inbox_b = b.acc.inbox().await.unwrap();
    assert!(
        a.acc
            .list_mailbox(&inbox_a, Page::default())
            .await
            .unwrap()
            .is_empty(),
        "nothing is delivered before the sweep runs"
    );

    // One sweep serves every tenant: each notification is built from the
    // claimed row's own context and delivered through that tenant's door.
    alo_jmap::site_booking_notify::run_due(&a.store).await;

    let listed_a = a.acc.list_mailbox(&inbox_a, Page::default()).await.unwrap();
    assert_eq!(listed_a.len(), 1, "owner A gets exactly one notification");
    let (headers_a, body_a) = open_message(&a.acc.message_bytes(&listed_a[0].id).await.unwrap());
    assert!(
        headers_a.contains("Subject: New booking: Consultation with Ada Lovelace (Harbour Studio)"),
        "the subject names what was booked and by whom: {headers_a}"
    );
    assert!(
        headers_a.contains("Reply-To: \"Ada Lovelace\" <ada@example.test>"),
        "the reply goes to the visitor: {headers_a}"
    );
    assert!(
        headers_a.contains(&format!("To: {}", a.email)),
        "addressed to the owner: {headers_a}"
    );
    assert!(body_a.contains("booked \"Consultation\""), "{body_a}");
    assert!(body_a.contains("Phone: +32 2 555 01"), "{body_a}");
    assert!(body_a.contains("already in your calendar"), "{body_a}");
    assert!(
        !body_a.contains("Grace Hopper") && !body_a.contains("Beta-only"),
        "tenant A must never see tenant B's appointment: {body_a}"
    );

    let listed_b = b.acc.list_mailbox(&inbox_b, Page::default()).await.unwrap();
    assert_eq!(listed_b.len(), 1, "owner B gets exactly one notification");
    let (_, body_b) = open_message(&b.acc.message_bytes(&listed_b[0].id).await.unwrap());
    assert!(body_b.contains("Beta-only fitting"), "{body_b}");
    assert!(
        !body_b.contains("Ada Lovelace"),
        "tenant B must never see tenant A's appointment: {body_b}"
    );

    // Claimed means notified: another sweep delivers nothing new to anyone.
    alo_jmap::site_booking_notify::run_due(&b.store).await;
    assert_eq!(
        a.acc
            .list_mailbox(&inbox_a, Page::default())
            .await
            .unwrap()
            .len(),
        1,
        "an appointment is never announced twice"
    );
    assert_eq!(
        b.acc
            .list_mailbox(&inbox_b, Page::default())
            .await
            .unwrap()
            .len(),
        1
    );
}
