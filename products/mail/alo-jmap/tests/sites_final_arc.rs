//! S2.16d — the wave's final arc, on the real local stack.
//!
//! One site travels the whole distance the S1 and S2 waves built, in one
//! sequence, across both services: an AI draft is **generated** from a
//! description (a scripted localhost fixture — never an external model),
//! **edited** through the authenticated editor surface (a catalog that takes
//! orders, a bookable service, both mounted as typed sections, a theme),
//! **published**, **served** anonymously by `alo-sites` on its own subdomain
//! Host, **converted** three ways by anonymous visitors (an enquiry, an order,
//! an appointment), delivered to the **owner's inbox** by the three sweeps, and
//! finally read back on the owner's **analytics and conversion** surfaces.
//!
//! Two properties are asserted at every stage rather than at the end, because
//! this is a wave review and not a demo:
//!
//! - **Tenancy.** A second tenant lives on the same store handle — the way
//!   production runs one process for every tenant — and is proven blind at each
//!   door: the site, its submissions, orders, analytics and conversions are all
//!   `404`, and its inbox stays empty while three notifications are delivered
//!   next door.
//! - **Budgets.** The published bytes an anonymous visitor downloads are
//!   measured, not assumed: the design note's page (< 100 KB) and stylesheet
//!   (< 50 KB) ceilings hold on a real generated site carrying every heavy
//!   section, and a warm cached page is served well inside a human's patience.
//!
//! One test on purpose: the notification sweeps are global, so scenarios
//! running concurrently in separate tests could claim (and deliver) each
//! other's rows mid-assertion.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::sync::{Arc, Mutex};
use std::time::Instant;

use alo_sites::serve::{AppState as PublicAppState, app as public_app};
use alo_store::{BlobStore, Page, SitePublicStore, TenantId, site_model};
use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode, header};
use base64::Engine;
use base64::engine::general_purpose::STANDARD as B64;
use serde_json::{Value, json};
use sqlx::postgres::PgPoolOptions;
use time::{Duration, OffsetDateTime};
use tower::ServiceExt;

use crate::common::{Harness, database_url, harness, harness_on, send};

/// The apex the public service serves under (`SITES_DOMAIN` in production).
const APEX: &str = "sites.test";

// ---------------------------------------------------------------------------
// The scripted model: a localhost HTTP fixture speaking the chat-completions
// shape. This suite never contacts an external AI service.
// ---------------------------------------------------------------------------

type Seen = Arc<Mutex<Vec<Value>>>;

async fn scripted_model(script: Vec<String>) -> (String, Seen) {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let seen: Seen = Arc::new(Mutex::new(Vec::new()));
    let record = Arc::clone(&seen);
    tokio::spawn(async move {
        loop {
            let Ok((mut socket, _)) = listener.accept().await else {
                return;
            };
            let record = Arc::clone(&record);
            let script = script.clone();
            tokio::spawn(async move {
                let mut buffer = Vec::new();
                let mut chunk = [0_u8; 8192];
                let body = loop {
                    let Ok(read) = socket.read(&mut chunk).await else {
                        return;
                    };
                    if read == 0 {
                        return;
                    }
                    buffer.extend_from_slice(&chunk[..read]);
                    let Some(end) = buffer.windows(4).position(|window| window == b"\r\n\r\n")
                    else {
                        continue;
                    };
                    let head = String::from_utf8_lossy(&buffer[..end]).into_owned();
                    let length = head
                        .lines()
                        .find_map(|line| {
                            line.to_ascii_lowercase()
                                .strip_prefix("content-length:")
                                .map(|value| value.trim().parse().unwrap_or(0))
                        })
                        .unwrap_or(0);
                    if buffer.len() >= end + 4 + length {
                        break buffer[end + 4..end + 4 + length].to_vec();
                    }
                };
                let turn = {
                    let mut requests = record.lock().unwrap();
                    requests.push(serde_json::from_slice(&body).unwrap_or(Value::Null));
                    requests.len() - 1
                };
                let content = script
                    .get(turn)
                    .or_else(|| script.last())
                    .cloned()
                    .unwrap_or_default();
                let answer = json!({
                    "choices": [{ "message": { "role": "assistant", "content": content } }]
                })
                .to_string();
                let response = format!(
                    "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
                    answer.len(),
                    answer
                );
                let _ = socket.write_all(response.as_bytes()).await;
                let _ = socket.flush().await;
            });
        }
    });
    (format!("http://{addr}"), seen)
}

async fn use_model(harness: &Harness, base_url: &str) {
    let id = format!("sites-arc-{}", harness.tenant);
    harness
        .acc
        .upsert_ai_provider(
            &id,
            "openai",
            "scripted",
            base_url,
            "fixture-model",
            None,
            true,
        )
        .await
        .unwrap();
    harness.acc.set_default_ai_provider(&id).await.unwrap();
}

fn valid_fixture(subdomain: &str) -> String {
    include_str!("../../../../platform/alo-ai/tests/fixtures/sites/valid_full_site.json")
        .replace("juniper-bakery", subdomain)
}

// ---------------------------------------------------------------------------
// Authenticated editor calls.
// ---------------------------------------------------------------------------

async fn post(app: &Router, token: &str, uri: &str, body: Value) -> (StatusCode, Value) {
    send(
        app,
        Request::builder()
            .method("POST")
            .uri(uri)
            .header("authorization", format!("Bearer {token}"))
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(body.to_string()))
            .unwrap(),
    )
    .await
}

async fn put(app: &Router, token: &str, uri: &str, body: Value) -> (StatusCode, Value) {
    send(
        app,
        Request::builder()
            .method("PUT")
            .uri(uri)
            .header("authorization", format!("Bearer {token}"))
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(body.to_string()))
            .unwrap(),
    )
    .await
}

async fn get(app: &Router, token: &str, uri: &str) -> (StatusCode, Value) {
    send(
        app,
        Request::builder()
            .uri(uri)
            .header("authorization", format!("Bearer {token}"))
            .body(Body::empty())
            .unwrap(),
    )
    .await
}

fn ok(step: &str, (status, body): (StatusCode, Value)) -> Value {
    assert_eq!(status, StatusCode::OK, "{step} failed: {body}");
    body
}

fn created_id(step: &str, response: (StatusCode, Value)) -> String {
    ok(step, response)["id"]
        .as_str()
        .expect("created id")
        .to_owned()
}

/// A subdomain nobody else on the shared local database owns.
fn subdomain(tag: &str, tenant: &TenantId) -> String {
    let salt: String = tenant
        .as_str()
        .chars()
        .filter(char::is_ascii_alphanumeric)
        .map(|character| character.to_ascii_lowercase())
        .take(20)
        .collect();
    format!("{tag}-{salt}")
}

// ---------------------------------------------------------------------------
// Anonymous visitor calls, through the separate public router.
// ---------------------------------------------------------------------------

/// A GET of a published path on the visitor's Host; returns status, headers'
/// content type, and the body bytes actually downloaded.
async fn visit(
    state: &Arc<PublicAppState>,
    host: &str,
    path: &str,
) -> (StatusCode, String, Vec<u8>) {
    let request = Request::builder()
        .uri(path)
        .header(header::HOST, host)
        .header("x-forwarded-for", "203.0.113.77")
        .header(header::USER_AGENT, "Mozilla/5.0 (final-arc visitor)")
        .header(
            header::REFERER,
            "https://news.example/weekend/guide?utm_source=post",
        )
        .body(Body::empty())
        .unwrap();
    let response = public_app(Arc::clone(state))
        .oneshot(request)
        .await
        .unwrap();
    let status = response.status();
    let content_type = response
        .headers()
        .get(header::CONTENT_TYPE)
        .map(|value| value.to_str().unwrap_or_default().to_owned())
        .unwrap_or_default();
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    (status, content_type, bytes.to_vec())
}

/// One urlencoded POST from an anonymous visitor (`/f/…`, `/o/…`, `/b/…`).
async fn submit(
    state: &Arc<PublicAppState>,
    host: &str,
    path: &str,
    client: &str,
    body: &str,
) -> (StatusCode, String) {
    let request = Request::builder()
        .method("POST")
        .uri(path)
        .header(header::HOST, host)
        .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
        .header("x-forwarded-for", client)
        .body(Body::from(body.to_owned()))
        .unwrap();
    let response = public_app(Arc::clone(state))
        .oneshot(request)
        .await
        .unwrap();
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    (status, String::from_utf8_lossy(&bytes).into_owned())
}

fn urlencode(value: &str) -> String {
    value
        .bytes()
        .map(|byte| match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                (byte as char).to_string()
            }
            b' ' => "+".to_owned(),
            other => format!("%{other:02X}"),
        })
        .collect()
}

/// Splits a raw RFC 5322 message into (headers, base64-decoded body text).
fn open_message(raw: &[u8]) -> (String, String) {
    let raw = String::from_utf8_lossy(raw).into_owned();
    let (headers, body) = raw.split_once("\r\n\r\n").expect("header/body split");
    let body = B64
        .decode(body.replace(['\r', '\n'], ""))
        .expect("base64 body");
    (
        headers.to_owned(),
        String::from_utf8(body).expect("utf-8 body"),
    )
}

/// The `value="…"` of the radio button whose label carries `clock`, on the
/// public day page — the exact instant the visitor's browser would post back.
fn slot_value(page: &str, clock: &str) -> String {
    let needle = format!(">{clock}<");
    assert!(page.contains(&needle), "the day offers {clock}: {page}");
    let before = page.split(&needle).next().unwrap();
    let value = before.rsplit("value=\"").next().unwrap();
    value.split('"').next().unwrap().to_owned()
}

#[tokio::test]
async fn a_generated_site_is_edited_published_served_converted_notified_and_reported() {
    let owner = harness("sites-arc-owner").await;
    // The outsider shares the store handle: production runs one process, one
    // blob store and one set of sweeps for every tenant.
    let outsider = harness_on(Arc::clone(&owner.store), "sites-arc-outsider").await;

    // -- 1. Generate ------------------------------------------------------
    let sub = subdomain("arc", &owner.tenant);
    let (model_url, seen) = scripted_model(vec![valid_fixture(&sub)]).await;
    use_model(&owner, &model_url).await;

    let generated = ok(
        "generate",
        post(
            &owner.app,
            &owner.token,
            "/sites/generate",
            json!({ "description": "A small Utrecht bakery with a contact page" }),
        )
        .await,
    );
    assert_eq!(generated["site"]["status"], "draft", "never auto-published");
    assert_eq!(generated["site"]["subdomain"], sub);
    assert_eq!(seen.lock().unwrap().len(), 1, "one model turn, no retry");

    let site = generated["site"]["id"].as_str().unwrap().to_owned();
    let pages = generated["pages"].as_array().unwrap().clone();
    let home = pages
        .iter()
        .find(|page| page["home"] == true)
        .expect("a home page");
    let home_id = home["id"].as_str().unwrap().to_owned();
    let contact = pages
        .iter()
        .find(|page| page["slug"] == "contact")
        .expect("the contact page");
    // The generated contact section carries a real tenant-owned form (S1.32a).
    let form_id = contact["sections"]["sections"]
        .as_array()
        .unwrap()
        .iter()
        .find(|section| section["type"] == "contact_form")
        .and_then(|section| section["form_id"].as_str())
        .expect("the generated contact section is linked to a form")
        .to_owned();

    // -- 2. Edit: a catalog that takes orders, and a bookable service ------
    let catalog = created_id(
        "create catalog",
        post(
            &owner.app,
            &owner.token,
            &format!("/sites/{site}/catalogs"),
            json!({ "name": "Saturday bake", "currency": "EUR", "ordersEnabled": true }),
        )
        .await,
    );
    let loaf = ok(
        "create item",
        post(
            &owner.app,
            &owner.token,
            &format!("/sites/{site}/catalogs/{catalog}/items"),
            json!({ "name": "Sourdough loaf", "price": "€4,50", "description": "Baked at six." }),
        )
        .await,
    );
    assert_eq!(loaf["priceCents"], 450, "money is integer cents");
    assert_eq!(loaf["slug"], "sourdough-loaf");

    let calendar = owner
        .acc
        .ensure_personal_calendar()
        .await
        .unwrap()
        .as_str()
        .to_owned();
    // Open every weekday morning, so the day the visitor picks is always one
    // the service offers regardless of when the suite runs.
    let hours: Vec<Value> = (1..=7)
        .map(|weekday| json!({ "weekday": weekday, "startMinute": 540, "endMinute": 720 }))
        .collect();
    let booking = created_id(
        "create booking service",
        post(
            &owner.app,
            &owner.token,
            &format!("/sites/{site}/bookings"),
            json!({
                "name": "Bakery tour",
                "description": "Half an hour, behind the ovens.",
                "calendarId": calendar,
                "timeZone": "Europe/Brussels",
                "durationMinutes": 30,
                "location": "The bakehouse",
                "hours": hours,
            }),
        )
        .await,
    );

    let with_catalog = ok(
        "add catalog section",
        post(
            &owner.app,
            &owner.token,
            &format!("/sites/{site}/pages/{home_id}/sections"),
            json!({ "section": {
                "type": "catalog",
                "catalog_id": catalog,
                "heading": "What we bake on Saturdays",
            }}),
        )
        .await,
    );
    assert!(
        with_catalog["sections"]["sections"]
            .as_array()
            .unwrap()
            .iter()
            .any(|section| section["type"] == "catalog"),
        "the catalog section joined the page: {with_catalog}"
    );
    ok(
        "add booking section",
        post(
            &owner.app,
            &owner.token,
            &format!("/sites/{site}/pages/{home_id}/sections"),
            json!({ "section": {
                "type": "booking",
                "booking_id": booking,
                "heading": "Come and see the ovens",
            }}),
        )
        .await,
    );
    ok(
        "set theme",
        put(
            &owner.app,
            &owner.token,
            &format!("/sites/{site}/theme"),
            json!({ "schema_version": 1, "preset": "midnight" }),
        )
        .await,
    );

    // -- 3. Publish -------------------------------------------------------
    let published = ok(
        "publish",
        post(
            &owner.app,
            &owner.token,
            &format!("/sites/{site}/publish"),
            json!({}),
        )
        .await,
    );
    assert_eq!(published["status"], "live");

    // -- 4. Serve: the anonymous visitor, on the site's own Host ----------
    let pool = PgPoolOptions::new()
        .max_connections(6)
        .connect(&database_url())
        .await
        .unwrap();
    let public = PublicAppState::new(
        SitePublicStore::new(pool, BlobStore::in_memory(4 * 1024 * 1024)),
        APEX.to_owned(),
        b"sites-final-arc-analytics-secret",
    );
    let host = format!("{sub}.{APEX}");

    let (status, content_type, bytes) = visit(&public, &host, "/").await;
    assert_eq!(status, StatusCode::OK, "the published home page is served");
    assert!(content_type.starts_with("text/html"), "{content_type}");
    let page = String::from_utf8(bytes.clone()).unwrap();
    assert!(
        page.contains("Bread worth waking up for"),
        "the generated hero"
    );
    assert!(
        page.contains("What we bake on Saturdays"),
        "the catalog section"
    );
    assert!(
        page.contains("Sourdough loaf") && page.contains("\u{a0}4.50"),
        "the price is frozen from the publish, not sent by the client"
    );
    assert!(
        page.contains("Come and see the ovens"),
        "the booking section"
    );
    assert!(
        page.contains(&format!("/b/{booking}")),
        "the booking form posts back"
    );

    let (status, _, contact_bytes) = visit(&public, &host, "/contact").await;
    assert_eq!(status, StatusCode::OK);
    let contact_page = String::from_utf8(contact_bytes.clone()).unwrap();
    assert!(
        contact_page.contains(&format!("/f/{form_id}")),
        "the enquiry form posts back to its own form id"
    );

    // Another tenant's Host can never reach this site (S1.09), and an
    // unknown subdomain is a plain 404 rather than a leak.
    let (status, _, _) = visit(&public, &format!("nobody-here.{APEX}"), "/").await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    // -- 4b. Byte and latency budgets, measured on the served bytes -------
    let css_path = "/assets/site.css";
    assert!(
        page.contains(css_path),
        "the page links the published stylesheet"
    );
    let (status, css_type, css_bytes) = visit(&public, &host, css_path).await;
    assert_eq!(status, StatusCode::OK);
    assert!(css_type.starts_with("text/css"), "{css_type}");
    assert!(
        bytes.len() < 100 * 1024,
        "the design note's page budget: home page is {} bytes",
        bytes.len()
    );
    assert!(
        contact_bytes.len() < 100 * 1024,
        "the design note's page budget: contact page is {} bytes",
        contact_bytes.len()
    );
    assert!(
        css_bytes.len() < 50 * 1024,
        "the design note's stylesheet budget: {} bytes",
        css_bytes.len()
    );
    // A warm page comes from the publish cache; the ceiling is deliberately
    // generous (a slow shared local Postgres must not make this flap) and is
    // still an order of magnitude under what a visitor would notice.
    let started = Instant::now();
    let (status, _, _) = visit(&public, &host, "/").await;
    let warm = started.elapsed();
    assert_eq!(status, StatusCode::OK);
    assert!(
        warm.as_millis() < 500,
        "a warm published page took {warm:?}"
    );

    // -- 5. Convert: an enquiry, an order, an appointment -----------------
    let (status, _) = submit(
        &public,
        &host,
        &format!("/f/{form_id}"),
        "203.0.113.10",
        "name=Ada+Lovelace&email=ada%40example.test&message=Do+you+deliver+to+Utrecht+Noord%3F&website=",
    )
    .await;
    assert_eq!(status, StatusCode::OK, "the enquiry is accepted");

    let (status, receipt) = submit(
        &public,
        &host,
        &format!("/o/{catalog}"),
        "203.0.113.11",
        "qty-sourdough-loaf=2&name=Grace+Hopper&email=grace%40example.test&note=Collecting+at+nine",
    )
    .await;
    assert_eq!(status, StatusCode::OK, "the order is accepted: {receipt}");
    assert!(receipt.contains("Order received"), "{receipt}");

    let day = (OffsetDateTime::now_utc().date() + Duration::days(7))
        .format(&time::macros::format_description!("[year]-[month]-[day]"))
        .unwrap();
    let (status, _, day_bytes) = visit(&public, &host, &format!("/b/{booking}?date={day}")).await;
    assert_eq!(status, StatusCode::OK);
    let day_page = String::from_utf8(day_bytes).unwrap();
    let slot = slot_value(&day_page, "09:30");
    let (status, confirmation) = submit(
        &public,
        &host,
        &format!("/b/{booking}"),
        "203.0.113.12",
        &format!(
            "slot={}&name=Alan+Turing&email=alan%40example.test&website=",
            urlencode(&slot)
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "the appointment is accepted");
    assert!(
        confirmation.contains("Appointment booked"),
        "{confirmation}"
    );

    // -- 6. The owner's inbox: three sweeps, three notifications ----------
    // Each sweep is global — it serves every tenant in the process, and on the
    // shared local database it may also carry rows another suite left behind.
    // What this arc pins is that ours are delivered, and that they are
    // delivered to exactly one inbox (asserted on both inboxes below).
    assert!(alo_jmap::site_notify::run_due(&owner.store).await >= 1);
    assert!(alo_jmap::site_order_notify::run_due(&owner.store).await >= 1);
    assert!(alo_jmap::site_booking_notify::run_due(&owner.store).await >= 1);

    let inbox = owner.acc.inbox().await.unwrap();
    let delivered = owner
        .acc
        .list_mailbox(&inbox, Page::default())
        .await
        .unwrap();
    assert_eq!(delivered.len(), 3, "one notification per conversion");
    let mut subjects: Vec<String> = Vec::new();
    for summary in &delivered {
        let raw = owner.acc.message_bytes(&summary.id).await.unwrap();
        let (headers, body) = open_message(&raw);
        assert!(
            headers.contains(&format!("To: {}", owner.email)),
            "addressed to the owner: {headers}"
        );
        subjects.push(summary.subject.clone());
        assert!(!body.is_empty());
    }
    assert!(
        subjects.iter().any(|s| s.contains("Ada Lovelace")),
        "the enquiry: {subjects:?}"
    );
    assert!(
        subjects.iter().any(|s| s.contains("Grace Hopper")),
        "the order: {subjects:?}"
    );
    assert!(
        subjects.iter().any(|s| s.contains("Alan Turing")),
        "the appointment: {subjects:?}"
    );

    // The appointment is an event in the owner's own Agenda calendar, not a
    // second copy of one kept by Sites — and it is nowhere near the outsider.
    let booked_day = OffsetDateTime::now_utc().date() + Duration::days(7);
    let from = booked_day.with_hms(0, 0, 0).unwrap().assume_utc();
    let events = owner
        .acc
        .events_in_range(from, from + Duration::days(1))
        .await
        .unwrap();
    assert_eq!(events.len(), 1, "one appointment on the booked day");
    assert!(events[0].summary.contains("Alan Turing"), "{events:?}");
    assert!(
        outsider
            .acc
            .events_in_range(from, from + Duration::days(1))
            .await
            .unwrap()
            .is_empty(),
        "the appointment never touches another tenant's calendar"
    );

    // -- 7. The owner reads the site back --------------------------------
    let submissions = ok(
        "submissions",
        get(
            &owner.app,
            &owner.token,
            &format!("/sites/{site}/submissions"),
        )
        .await,
    );
    assert_eq!(submissions["submissions"].as_array().unwrap().len(), 1);
    assert_eq!(submissions["submissions"][0]["senderName"], "Ada Lovelace");

    let orders = ok(
        "orders",
        get(&owner.app, &owner.token, &format!("/sites/{site}/orders")).await,
    );
    assert_eq!(orders["orders"].as_array().unwrap().len(), 1);
    assert_eq!(orders["orders"][0]["totalCents"], 900);

    let report = ok(
        "analytics",
        get(
            &owner.app,
            &owner.token,
            &format!("/sites/{site}/analytics?days=7"),
        )
        .await,
    );
    assert!(
        report["totals"]["visits"].as_i64().unwrap() >= 3,
        "every anonymous page view is counted: {report}"
    );
    let paths: Vec<&str> = report["topPages"]
        .as_array()
        .unwrap()
        .iter()
        .map(|entry| entry["path"].as_str().unwrap())
        .collect();
    assert!(paths.contains(&"/"), "the home page is ranked: {paths:?}");
    assert!(
        paths.contains(&"/contact"),
        "so is the contact page: {paths:?}"
    );
    // The visitor sent a raw IP, a user agent, a referrer path and a query
    // token; the report may keep the referrer's DOMAIN and nothing else of it.
    let serialized = report.to_string();
    for leak in [
        "203.0.113.77",
        "Mozilla",
        "weekend/guide",
        "utm_source=post",
    ] {
        assert!(
            !serialized.contains(leak),
            "the traffic report must never carry {leak}: {serialized}"
        );
    }

    let funnel = ok(
        "conversions",
        get(
            &owner.app,
            &owner.token,
            &format!("/sites/{site}/conversions?days=7"),
        )
        .await,
    );
    assert!(
        funnel["totals"]["submits"].as_i64().unwrap() >= 1,
        "the enquiry is counted where it was written: {funnel}"
    );
    assert!(
        funnel["sources"]
            .as_array()
            .unwrap()
            .iter()
            .any(|source| source["id"] == form_id.as_str() && source["submits"].as_i64() == Some(1)),
        "the form's own counter: {funnel}"
    );

    // -- 8. The outsider, at every door ----------------------------------
    for door in [
        format!("/sites/{site}"),
        format!("/sites/{site}/pages"),
        format!("/sites/{site}/submissions"),
        format!("/sites/{site}/orders"),
        format!("/sites/{site}/catalogs"),
        format!("/sites/{site}/bookings"),
        format!("/sites/{site}/analytics?days=7"),
        format!("/sites/{site}/conversions?days=7"),
        format!("/sites/{site}/publishes"),
    ] {
        let (status, body) = get(&outsider.app, &outsider.token, &door).await;
        assert_eq!(
            status,
            StatusCode::NOT_FOUND,
            "another tenant reached {door}: {body}"
        );
    }
    let outsider_inbox = outsider.acc.inbox().await.unwrap();
    assert!(
        outsider
            .acc
            .list_mailbox(&outsider_inbox, Page::default())
            .await
            .unwrap()
            .is_empty(),
        "three notifications were delivered and none of them next door"
    );

    // The published sections are exactly the typed vocabulary — the arc never
    // wrote a shape the renderer would have to guess at.
    let stored = ok(
        "page read-back",
        get(
            &owner.app,
            &owner.token,
            &format!("/sites/{site}/pages/{home_id}"),
        )
        .await,
    );
    let sections: site_model::SectionsEnvelope =
        serde_json::from_value(stored["sections"].clone()).expect("typed sections");
    assert!(
        sections
            .sections
            .iter()
            .any(|section| section.kind() == "catalog")
    );
    assert!(
        sections
            .sections
            .iter()
            .any(|section| section.kind() == "booking")
    );
}
