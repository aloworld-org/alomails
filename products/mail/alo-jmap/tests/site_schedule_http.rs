//! The `/sites/{id}/schedule` surface and the sweep behind it (ADR 0036,
//! S2.05b), driven through the real router over a real Postgres.
//!
//! `alo-store`'s own suite proves the model (concurrency, wrong tenant, the
//! stale-claim path). What this pins is the edge: the auth guard, the exact
//! JSON the visible schedule control reads, the refusals a person can act on,
//! and the two ends the worker writes — a website that went live by itself,
//! and one that refused with the reason kept for its owner.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use crate::common;

use std::sync::Arc;
use std::time::Duration;

use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use serde_json::{Value, json};
use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;

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

async fn delete(app: &Router, token: &str, uri: &str) -> (StatusCode, Value) {
    send(app, with_json("DELETE", uri, Some(token), Value::Null)).await
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

/// A moment truncated to the microsecond — the finest one Postgres keeps.
///
/// `timestamptz` stores microseconds, while `OffsetDateTime::now_utc()` on this
/// platform carries finer precision than that. A moment taken straight from the
/// clock therefore comes back from a round trip a few hundred nanoseconds
/// shorter than it went in, and comparing the two fails — except on the roughly
/// one run in ten where the clock lands on a whole microsecond, which is how
/// this read as flakiness rather than as the truncation it is.
///
/// Truncating here rather than comparing loosely at the assertion keeps the
/// test's claim exact: the moment we asked for is the moment that comes back,
/// with no tolerance to hide a real drift.
fn to_micros(moment: OffsetDateTime) -> OffsetDateTime {
    let micros = moment.nanosecond() / 1_000 * 1_000;
    moment
        .replace_nanosecond(micros)
        .expect("a truncated nanosecond is always in range")
}

/// A subdomain unique to this harness run — the namespace is global.
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

/// Creates a draft site. With `home`, it also gets the home page that makes it
/// publishable; without, publishing it must refuse.
async fn draft_site(h: &Harness, tag: &str, home: bool) -> String {
    let (status, site) = post(
        &h.app,
        &h.token,
        "/sites",
        json!({ "name": "Roastery", "subdomain": sub(tag, h) }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "create site: {site}");
    let site_id = site["id"].as_str().unwrap().to_owned();
    if home {
        let (status, page) = post(
            &h.app,
            &h.token,
            &format!("/sites/{site_id}/pages"),
            json!({ "title": "Home", "slug": "", "home": true }),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "create page: {page}");
        let page_id = page["id"].as_str().unwrap().to_owned();
        let (status, body) = post(
            &h.app,
            &h.token,
            &format!("/sites/{site_id}/pages/{page_id}/sections"),
            json!({ "section": { "type": "hero", "heading": "Sourdough, daily" } }),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "add hero: {body}");
    }
    site_id
}

fn instant(value: &Value) -> OffsetDateTime {
    OffsetDateTime::parse(value.as_str().expect("a timestamp"), &Rfc3339).expect("RFC 3339")
}

#[tokio::test]
async fn schedule_routes_require_a_bearer_token() {
    let h = harness("site-schedule-401").await;
    let attempts = [
        ("GET", "/sites/some-id/schedule", None),
        (
            "POST",
            "/sites/some-id/schedule",
            Some(json!({ "publishAt": "2030-01-01T09:00:00Z" })),
        ),
        ("DELETE", "/sites/some-id/schedule/some-schedule", None),
    ];
    for (method, uri, body) in attempts {
        let (status, problem) = send(
            &h.app,
            with_json(method, uri, None, body.unwrap_or(Value::Null)),
        )
        .await;
        assert_eq!(
            status,
            StatusCode::UNAUTHORIZED,
            "{method} {uri}: {problem}"
        );
    }
}

/// The whole arc the schedule control needs: nothing scheduled, a moment
/// chosen, the same moment moved, and the intention called off — with the
/// refusals a person can read on the way.
#[tokio::test]
async fn a_publish_is_scheduled_moved_and_called_off() {
    let h = harness("site-schedule").await;
    let site = draft_site(&h, "sched", true).await;

    // Nothing scheduled is an answer, not a failure.
    let (status, body) = get(&h.app, &h.token, &format!("/sites/{site}/schedule")).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["schedule"], Value::Null);
    assert_eq!(body["history"], json!([]));

    // A moment that has gone, and one that is not a moment at all.
    let (status, problem) = post(
        &h.app,
        &h.token,
        &format!("/sites/{site}/schedule"),
        json!({ "publishAt": "2020-01-01T09:00:00Z" }),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(
        problem["detail"],
        json!("a scheduled publish must be in the future")
    );
    let (status, problem) = post(
        &h.app,
        &h.token,
        &format!("/sites/{site}/schedule"),
        json!({ "publishAt": "next monday" }),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert!(
        problem["detail"]
            .as_str()
            .unwrap()
            .contains("2026-09-01T09:00:00+02:00"),
        "the refusal shows the shape it wants: {problem}"
    );

    // 09:00 in Amsterdam is a moment, not a string: it comes back in UTC.
    let chosen = to_micros(OffsetDateTime::now_utc() + time::Duration::days(2));
    let (status, scheduled) = post(
        &h.app,
        &h.token,
        &format!("/sites/{site}/schedule"),
        json!({ "publishAt": chosen.format(&Rfc3339).unwrap() }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{scheduled}");
    let schedule_id = scheduled["id"].as_str().unwrap().to_owned();
    assert_eq!(scheduled["status"], json!("scheduled"));
    assert_eq!(scheduled["siteId"], json!(site));
    assert_eq!(scheduled["requestedBy"], json!(h.user.as_str()));
    assert_eq!(scheduled["attempts"], json!(0));
    assert_eq!(scheduled["publishId"], Value::Null);
    assert_eq!(scheduled["lastError"], Value::Null);
    assert_eq!(scheduled["finishedAt"], Value::Null);
    assert_eq!(instant(&scheduled["publishAt"]), chosen);
    assert_eq!(
        instant(&scheduled["publishAt"]).offset(),
        time::UtcOffset::UTC,
        "the wire reports UTC; the screen turns it back into the reader's time"
    );

    // Moving the moment keeps the id, so a surface watching one schedule keeps
    // watching it — and there is still exactly one intention.
    let moved_to = to_micros(OffsetDateTime::now_utc() + time::Duration::days(3));
    let (status, moved) = post(
        &h.app,
        &h.token,
        &format!("/sites/{site}/schedule"),
        json!({ "publishAt": moved_to.format(&Rfc3339).unwrap() }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{moved}");
    assert_eq!(moved["id"], json!(schedule_id));
    assert_eq!(instant(&moved["publishAt"]), moved_to);

    let (status, body) = get(&h.app, &h.token, &format!("/sites/{site}/schedule")).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["schedule"]["id"], json!(schedule_id));
    assert_eq!(body["history"].as_array().unwrap().len(), 1);

    // Called off: the pending read goes quiet, the row stays readable.
    let (status, cancelled) = delete(
        &h.app,
        &h.token,
        &format!("/sites/{site}/schedule/{schedule_id}"),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{cancelled}");
    assert_eq!(cancelled["status"], json!("cancelled"));
    assert!(cancelled["finishedAt"].is_string());

    let (status, body) = get(&h.app, &h.token, &format!("/sites/{site}/schedule")).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["schedule"], Value::Null);
    assert_eq!(body["history"].as_array().unwrap().len(), 1);
    assert_eq!(body["history"][0]["status"], json!("cancelled"));

    // Cancelling twice is a sentence, not a stack trace.
    let (status, problem) = delete(
        &h.app,
        &h.token,
        &format!("/sites/{site}/schedule/{schedule_id}"),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(
        problem["detail"],
        json!("this scheduled publish has already finished")
    );

    // An id that never existed, under a site that does.
    let (status, problem) = delete(
        &h.app,
        &h.token,
        &format!("/sites/{site}/schedule/not-a-schedule"),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND, "{problem}");
    assert_eq!(
        problem["detail"],
        json!("no such scheduled publish for this website")
    );

    // The site never went live: scheduling is an intention, not a publish.
    let (status, detail) = get(&h.app, &h.token, &format!("/sites/{site}")).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(detail["status"], json!("draft"));
}

/// Another tenant cannot read the schedule, move it, or call it off — and
/// every refusal is the same `404` an invented id gets.
#[tokio::test]
async fn another_tenant_cannot_read_or_touch_a_schedule() {
    let (h, _blobs) = common::harness_with_blobs("site-schedule-owner").await;
    let outsider = harness_on(Arc::clone(&h.store), "site-schedule-outsider").await;
    let site = draft_site(&h, "own", true).await;
    let at = OffsetDateTime::now_utc() + time::Duration::days(1);
    let (status, scheduled) = post(
        &h.app,
        &h.token,
        &format!("/sites/{site}/schedule"),
        json!({ "publishAt": at.format(&Rfc3339).unwrap() }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{scheduled}");
    let schedule_id = scheduled["id"].as_str().unwrap().to_owned();

    let (status, problem) = get(
        &outsider.app,
        &outsider.token,
        &format!("/sites/{site}/schedule"),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND, "{problem}");
    assert_eq!(problem["detail"], json!("no such site"));

    let (status, problem) = post(
        &outsider.app,
        &outsider.token,
        &format!("/sites/{site}/schedule"),
        json!({ "publishAt": at.format(&Rfc3339).unwrap() }),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND, "{problem}");

    let (status, problem) = delete(
        &outsider.app,
        &outsider.token,
        &format!("/sites/{site}/schedule/{schedule_id}"),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND, "{problem}");

    // Naming the foreign schedule under a site the outsider does own is the
    // same answer: the id is not an oracle.
    let outsider_site = draft_site(&outsider, "out", true).await;
    let (status, problem) = delete(
        &outsider.app,
        &outsider.token,
        &format!("/sites/{outsider_site}/schedule/{schedule_id}"),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND, "{problem}");
    assert_eq!(
        problem["detail"],
        json!("no such scheduled publish for this website")
    );

    // Untouched: the owner's intention is still exactly where it was.
    let (status, body) = get(&h.app, &h.token, &format!("/sites/{site}/schedule")).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["schedule"]["id"], json!(schedule_id));
    assert_eq!(body["schedule"]["status"], json!("scheduled"));
}

/// The moment arrives: one website goes live by itself, and one that cannot be
/// published keeps the reason where its owner will read it.
#[tokio::test]
async fn a_due_schedule_publishes_and_a_refusal_is_kept_for_the_owner() {
    let h = harness("site-schedule-sweep").await;
    let ready = draft_site(&h, "due", true).await;
    let empty = draft_site(&h, "nohome", false).await;

    let at = OffsetDateTime::now_utc() + time::Duration::seconds(1);
    for site in [&ready, &empty] {
        let (status, scheduled) = post(
            &h.app,
            &h.token,
            &format!("/sites/{site}/schedule"),
            json!({ "publishAt": at.format(&Rfc3339).unwrap() }),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{scheduled}");
    }

    // Nothing is due yet, so the sweep leaves both intentions alone.
    alo_jmap::site_publish_worker::run_due(&h.store).await;
    let (_, before) = get(&h.app, &h.token, &format!("/sites/{ready}/schedule")).await;
    assert_eq!(before["schedule"]["status"], json!("scheduled"));

    tokio::time::sleep(Duration::from_millis(1500)).await;
    let published = alo_jmap::site_publish_worker::run_due(&h.store).await;
    assert!(published >= 1, "the due website went live");

    let (status, body) = get(&h.app, &h.token, &format!("/sites/{ready}/schedule")).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["schedule"], Value::Null, "nothing is pending anymore");
    let done = &body["history"][0];
    assert_eq!(done["status"], json!("published"));
    assert_eq!(done["attempts"], json!(1));
    assert!(
        done["publishId"].is_string(),
        "it names the version: {done}"
    );
    assert_eq!(done["lastError"], Value::Null);
    assert!(done["finishedAt"].is_string());

    // It is really on the internet, and the version records the person who
    // scheduled it as its author.
    let (status, detail) = get(&h.app, &h.token, &format!("/sites/{ready}")).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(detail["status"], json!("live"));
    assert_eq!(detail["publish"]["id"], done["publishId"]);
    let (status, versions) = get(&h.app, &h.token, &format!("/sites/{ready}/publishes")).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(versions["current"], done["publishId"]);
    assert_eq!(
        versions["publishes"][0]["publishedBy"],
        json!(h.user.as_str())
    );

    // The site that could not be published says why, in words, once — a
    // refusal is terminal, not retried until the attempts run out.
    let (status, body) = get(&h.app, &h.token, &format!("/sites/{empty}/schedule")).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["schedule"], Value::Null);
    let failed = &body["history"][0];
    assert_eq!(failed["status"], json!("failed"));
    assert_eq!(failed["lastError"], json!("site has no pages to publish"));
    assert_eq!(failed["attempts"], json!(1));
    assert_eq!(failed["publishId"], Value::Null);
    let (status, detail) = get(&h.app, &h.token, &format!("/sites/{empty}")).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(detail["status"], json!("draft"));

    // A second sweep has nothing left to do with either of them.
    alo_jmap::site_publish_worker::run_due(&h.store).await;
    let (_, body) = get(&h.app, &h.token, &format!("/sites/{empty}/schedule")).await;
    assert_eq!(body["history"].as_array().unwrap().len(), 1);
    assert_eq!(body["history"][0]["attempts"], json!(1));

    // The moment having passed, the site can be scheduled again.
    let again = OffsetDateTime::now_utc() + time::Duration::days(1);
    let (status, scheduled) = post(
        &h.app,
        &h.token,
        &format!("/sites/{empty}/schedule"),
        json!({ "publishAt": again.format(&Rfc3339).unwrap() }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{scheduled}");
    assert_eq!(scheduled["status"], json!("scheduled"));
}
