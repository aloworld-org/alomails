//! End-to-end CalDAV (RFC 4791): the request sequence a real client runs —
//! OPTIONS, principal/calendar-home discovery via PROPFIND, PUT to create,
//! GET, listing, `calendar-multiget` + `calendar-query` (time-range) +
//! `sync-collection` REPORTs, preconditions, DELETE — plus the mandatory
//! isolation proofs **per method**: wrong tenant, wrong user in the same
//! tenant, and the read-only shared calendar (viewer grant) whose writes are
//! refused with `403`, never misfiled and never a 500. Driven through the
//! real router over Postgres, mirroring `carddav.rs`.
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use base64::Engine;
use common::{Harness, harness, harness_on};

fn basic(email: &str) -> String {
    let raw = format!("{email}:s3cret-pw");
    format!(
        "Basic {}",
        base64::engine::general_purpose::STANDARD.encode(raw)
    )
}

/// Sends a DAV request (any method) authenticated as `email` through `h`'s
/// router; returns status, headers, body.
async fn dav_as(
    h: &Harness,
    email: &str,
    method: &str,
    path: &str,
    depth: Option<&str>,
    body: &str,
) -> (StatusCode, axum::http::HeaderMap, String) {
    let mut b = Request::builder()
        .method(method)
        .uri(path)
        .header("authorization", basic(email));
    if let Some(d) = depth {
        b = b.header("depth", d);
    }
    let req = b.body(Body::from(body.to_owned())).unwrap();
    let resp = {
        use tower::ServiceExt;
        h.app.clone().oneshot(req).await.unwrap()
    };
    let status = resp.status();
    let headers = resp.headers().clone();
    let bytes = axum::body::to_bytes(resp.into_body(), 16 * 1024 * 1024)
        .await
        .unwrap();
    (
        status,
        headers,
        String::from_utf8_lossy(&bytes).into_owned(),
    )
}

async fn dav(
    h: &Harness,
    method: &str,
    path: &str,
    depth: Option<&str>,
    body: &str,
) -> (StatusCode, axum::http::HeaderMap, String) {
    dav_as(h, &h.email, method, path, depth, body).await
}

/// A minimal client-authored VEVENT at a UTC hour of 2026.
fn ics(uid: &str, summary: &str, start: &str, end: &str) -> String {
    format!(
        "BEGIN:VCALENDAR\r\nVERSION:2.0\r\nPRODID:-//Test//caldav//EN\r\n\
         BEGIN:VEVENT\r\nUID:{uid}\r\nDTSTAMP:20260101T000000Z\r\n\
         DTSTART:{start}\r\nDTEND:{end}\r\nSUMMARY:{summary}\r\n\
         END:VEVENT\r\nEND:VCALENDAR\r\n"
    )
}

#[tokio::test]
async fn full_client_sync_flow() {
    let h = harness("caldav-flow").await;
    let uid = &h.account_id;

    // OPTIONS advertises CalDAV.
    let (status, headers, _) = dav(&h, "OPTIONS", "/dav/", None, "").await;
    assert_eq!(status, StatusCode::OK);
    assert!(
        headers
            .get("dav")
            .unwrap()
            .to_str()
            .unwrap()
            .contains("calendar-access"),
        "DAV: header advertises calendar-access"
    );

    // Principal discovery names the calendar home.
    let (status, _h, xml) = dav(
        &h,
        "PROPFIND",
        &format!("/dav/principals/{uid}/"),
        Some("0"),
        "",
    )
    .await;
    assert_eq!(status, StatusCode::MULTI_STATUS);
    assert!(xml.contains("calendar-home-set"), "{xml}");
    assert!(xml.contains(&format!("/dav/calendars/{uid}/")), "{xml}");

    // Calendar home (Depth 1) lists the personal calendar as `default/`.
    let (status, _h, xml) = dav(
        &h,
        "PROPFIND",
        &format!("/dav/calendars/{uid}/"),
        Some("1"),
        "",
    )
    .await;
    assert_eq!(status, StatusCode::MULTI_STATUS);
    assert!(xml.contains("<cal:calendar/>"), "{xml}");
    assert!(
        xml.contains(&format!("/dav/calendars/{uid}/default/")),
        "{xml}"
    );
    assert!(xml.contains("supported-report-set"), "{xml}");

    // PUT creates an object at a client-chosen href (unique per run: the
    // event id is the iCalendar UID, embedded with the account id so reruns
    // on the shared dev DB never collide).
    let cal = format!("/dav/calendars/{uid}/default/");
    let e1 = format!("june-{uid}");
    let e1_path = format!("{cal}{e1}.ics");
    let (status, headers, _) = dav(
        &h,
        "PUT",
        &e1_path,
        None,
        &ics(&e1, "June planning", "20260610T090000Z", "20260610T100000Z"),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "first PUT creates");
    let etag = headers.get("etag").unwrap().to_str().unwrap().to_owned();
    assert!(!etag.is_empty());

    // GET returns the iCalendar with the same ETag.
    let (status, headers, body) = dav(&h, "GET", &e1_path, None, "").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        headers.get("content-type").unwrap().to_str().unwrap(),
        "text/calendar; charset=utf-8"
    );
    assert!(body.contains("SUMMARY:June planning"), "{body}");
    assert!(body.contains("DTSTART:20260610T090000Z"), "{body}");
    assert_eq!(headers.get("etag").unwrap().to_str().unwrap(), etag);

    // Depth:1 on the collection lists the object with its etag.
    let (_s, _h, xml) = dav(&h, "PROPFIND", &cal, Some("1"), "").await;
    assert!(xml.contains(&format!("{e1}.ics")), "{xml}");
    assert!(xml.contains("getetag"), "{xml}");

    // calendar-multiget returns the embedded calendar-data.
    let multiget = format!(
        "<c:calendar-multiget xmlns:d=\"DAV:\" xmlns:c=\"urn:ietf:params:xml:ns:caldav\">\
         <d:href>{e1_path}</d:href></c:calendar-multiget>"
    );
    let (status, _h, xml) = dav(&h, "REPORT", &cal, Some("1"), &multiget).await;
    assert_eq!(status, StatusCode::MULTI_STATUS);
    assert!(xml.contains("calendar-data"), "{xml}");
    assert!(xml.contains("SUMMARY:June planning"), "{xml}");

    // A second event in December, then a calendar-query narrowed to June:
    // only the June event comes back.
    let e2 = format!("dec-{uid}");
    let e2_path = format!("{cal}{e2}.ics");
    let (status, ..) = dav(
        &h,
        "PUT",
        &e2_path,
        None,
        &ics(
            &e2,
            "Year-end close",
            "20261215T140000Z",
            "20261215T150000Z",
        ),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let query = "<c:calendar-query xmlns:d=\"DAV:\" xmlns:c=\"urn:ietf:params:xml:ns:caldav\">\
         <c:filter><c:comp-filter name=\"VCALENDAR\"><c:comp-filter name=\"VEVENT\">\
         <c:time-range start=\"20260601T000000Z\" end=\"20260701T000000Z\"/>\
         </c:comp-filter></c:comp-filter></c:filter></c:calendar-query>";
    let (status, _h, xml) = dav(&h, "REPORT", &cal, Some("1"), query).await;
    assert_eq!(status, StatusCode::MULTI_STATUS);
    assert!(xml.contains(&format!("{e1}.ics")), "June is inside: {xml}");
    assert!(
        !xml.contains(&format!("{e2}.ics")),
        "December is outside: {xml}"
    );

    // sync-collection (initial, empty token) returns both and a fresh token.
    let sync_init = "<d:sync-collection xmlns:d=\"DAV:\"><d:sync-token/></d:sync-collection>";
    let (status, _h, xml) = dav(&h, "REPORT", &cal, None, sync_init).await;
    assert_eq!(status, StatusCode::MULTI_STATUS);
    assert!(xml.contains(&format!("{e1}.ics")), "{xml}");
    assert!(xml.contains(&format!("{e2}.ics")), "{xml}");
    let token_start = xml.find("<d:sync-token>urn:alo:calendar:").expect("token");
    let token = &xml[token_start + "<d:sync-token>".len()..];
    let token = &token[..token.find("</d:sync-token>").unwrap()];

    // A write after the token: the incremental sync carries only the change.
    let e3 = format!("extra-{uid}");
    let e3_path = format!("{cal}{e3}.ics");
    let (status, ..) = dav(
        &h,
        "PUT",
        &e3_path,
        None,
        &ics(&e3, "Retro", "20260620T160000Z", "20260620T170000Z"),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let sync_incr = format!(
        "<d:sync-collection xmlns:d=\"DAV:\"><d:sync-token>{token}</d:sync-token></d:sync-collection>"
    );
    let (_s, _h, xml) = dav(&h, "REPORT", &cal, None, &sync_incr).await;
    assert!(xml.contains(&format!("{e3}.ics")), "{xml}");
    assert!(
        !xml.contains(&format!("{e1}.ics")),
        "unchanged objects stay out of an incremental sync: {xml}"
    );
    let t2_start = xml.find("<d:sync-token>urn:alo:calendar:").expect("token2");
    let token2 = &xml[t2_start + "<d:sync-token>".len()..];
    let token2 = token2[..token2.find("</d:sync-token>").unwrap()].to_owned();

    // DELETE surfaces as a 404 member in the next incremental sync. (From the
    // *newer* token — an object created and destroyed within one window is a
    // net no-op the change log deliberately omits.)
    let (status, ..) = dav(&h, "DELETE", &e3_path, None, "").await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    let sync_after = format!(
        "<d:sync-collection xmlns:d=\"DAV:\"><d:sync-token>{token2}</d:sync-token></d:sync-collection>"
    );
    let (_s, _h, xml) = dav(&h, "REPORT", &cal, None, &sync_after).await;
    assert!(
        xml.contains(&format!("{e3}.ics")) && xml.contains("404"),
        "the removal is reported: {xml}"
    );

    // Preconditions: create-only PUT on an existing object is refused, and a
    // stale If-Match blocks both PUT and DELETE.
    let (status, ..) = dav_precondition(&h, &e1_path, "if-none-match", "*").await;
    assert_eq!(status, StatusCode::PRECONDITION_FAILED);
    let (status, ..) = dav_precondition(&h, &e1_path, "if-match", "\"stale\"").await;
    assert_eq!(status, StatusCode::PRECONDITION_FAILED);
    let req = Request::builder()
        .method("DELETE")
        .uri(&e1_path)
        .header("authorization", basic(&h.email))
        .header("if-match", "\"stale\"")
        .body(Body::empty())
        .unwrap();
    let resp = {
        use tower::ServiceExt;
        h.app.clone().oneshot(req).await.unwrap()
    };
    assert_eq!(resp.status(), StatusCode::PRECONDITION_FAILED);

    // An honest DELETE with the current etag succeeds; GET then 404s.
    let (status, ..) = dav(&h, "DELETE", &e1_path, None, "").await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    let (status, ..) = dav(&h, "GET", &e1_path, None, "").await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

/// A PUT of `e1`'s content with one precondition header set.
async fn dav_precondition(
    h: &Harness,
    path: &str,
    header: &str,
    value: &str,
) -> (StatusCode, axum::http::HeaderMap, String) {
    let body = ics("pre", "Pre", "20260610T090000Z", "20260610T100000Z");
    let req = Request::builder()
        .method("PUT")
        .uri(path)
        .header("authorization", basic(&h.email))
        .header(header, value)
        .body(Body::from(body))
        .unwrap();
    let resp = {
        use tower::ServiceExt;
        h.app.clone().oneshot(req).await.unwrap()
    };
    let status = resp.status();
    let headers = resp.headers().clone();
    let bytes = axum::body::to_bytes(resp.into_body(), 1024 * 1024)
        .await
        .unwrap();
    (
        status,
        headers,
        String::from_utf8_lossy(&bytes).into_owned(),
    )
}

/// Wrong tenant: every method, addressed both through the victim's paths and
/// through the prober's own paths carrying the victim's ids, is a clean 404 —
/// never data, never a 500.
#[tokio::test]
async fn caldav_is_tenant_isolated_per_method() {
    let a = harness("caldav-iso-a").await;
    let b = harness_on(std::sync::Arc::clone(&a.store), "caldav-iso-b").await;
    let (ua, ub) = (&a.account_id, &b.account_id);

    let e = format!("secret-{ua}");
    let a_obj = format!("/dav/calendars/{ua}/default/{e}.ics");
    let (status, ..) = dav(
        &a,
        "PUT",
        &a_obj,
        None,
        &ics(
            &e,
            "Tenant A secret",
            "20260610T090000Z",
            "20260610T100000Z",
        ),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);

    // B through A's paths: the path user is not B → NotFound for every method.
    for method in ["PROPFIND", "REPORT", "GET", "PUT", "DELETE"] {
        let (status, _h, body) = dav_as(&a, &b.email, method, &a_obj, Some("1"), "").await;
        assert_eq!(status, StatusCode::NOT_FOUND, "{method} via A's path");
        assert!(!body.contains("Tenant A secret"), "{method} leaked: {body}");
    }

    // B through its own paths carrying A's ids: object GET/DELETE 404, the
    // collection REPORTs stay empty, a PUT into A's personal calendar id 404s.
    let b_obj = format!("/dav/calendars/{ub}/default/{e}.ics");
    assert_eq!(
        dav(&b, "GET", &b_obj, None, "").await.0,
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        dav(&b, "DELETE", &b_obj, None, "").await.0,
        StatusCode::NOT_FOUND
    );
    let foreign_coll = format!("/dav/calendars/{ub}/cal_personal_{ua}/");
    let (status, _h, xml) = dav(&b, "PROPFIND", &foreign_coll, Some("1"), "").await;
    assert_eq!(status, StatusCode::NOT_FOUND, "foreign collection PROPFIND");
    let multiget = format!(
        "<c:calendar-multiget xmlns:d=\"DAV:\" xmlns:c=\"urn:ietf:params:xml:ns:caldav\">\
         <d:href>{a_obj}</d:href></c:calendar-multiget>"
    );
    let (_s, _h, xml2) = dav(&b, "REPORT", &foreign_coll, Some("1"), &multiget).await;
    assert!(
        !xml.contains("Tenant A secret") && !xml2.contains("Tenant A secret"),
        "REPORT must not leak: {xml2}"
    );
    let (status, ..) = dav(
        &b,
        "PUT",
        &format!("/dav/calendars/{ub}/cal_personal_{ua}/x-{ub}.ics"),
        None,
        &ics(
            &format!("x-{ub}"),
            "Injected",
            "20260610T090000Z",
            "20260610T100000Z",
        ),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND, "PUT into a foreign calendar");

    // A still holds its object, untouched.
    let (status, _h, body) = dav(&a, "GET", &a_obj, None, "").await;
    assert_eq!(status, StatusCode::OK);
    assert!(body.contains("Tenant A secret"));
}

/// Wrong user, same tenant: the personal calendar of one user is invisible to
/// a colleague through every method — same-tenant is not same-account.
#[tokio::test]
async fn caldav_is_account_isolated_within_a_tenant() {
    let a = harness("caldav-acct").await;
    let ua = &a.account_id;

    // A colleague in the SAME tenant, with a legacy-auth password.
    let mate_email = format!("mate-{}@example.test", a.tenant);
    let mate = a.ts.create_user(&mate_email).await.unwrap();
    a.identity
        .set_password(&a.tenant, &mate, &mate_email, "s3cret-pw")
        .await
        .unwrap();

    let e = format!("private-{ua}");
    let a_obj = format!("/dav/calendars/{ua}/default/{e}.ics");
    let (status, ..) = dav(
        &a,
        "PUT",
        &a_obj,
        None,
        &ics(&e, "Salary talk", "20260610T090000Z", "20260610T100000Z"),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);

    // Through A's paths: 404 for every method (the path user is not the mate).
    for method in ["PROPFIND", "REPORT", "GET", "PUT", "DELETE"] {
        let (status, _h, body) = dav_as(&a, &mate_email, method, &a_obj, Some("1"), "").await;
        assert_eq!(status, StatusCode::NOT_FOUND, "{method} via A's path");
        assert!(!body.contains("Salary talk"), "{method} leaked: {body}");
    }

    // Through the mate's own paths, naming A's personal calendar id: the
    // collection 404s on PROPFIND, REPORTs return no members, GET/DELETE of
    // A's event id 404, and a PUT is refused unmisfiled.
    let mu = mate.to_string();
    let foreign_coll = format!("/dav/calendars/{mu}/cal_personal_{ua}/");
    assert_eq!(
        dav_as(&a, &mate_email, "PROPFIND", &foreign_coll, Some("1"), "")
            .await
            .0,
        StatusCode::NOT_FOUND
    );
    let sync = "<d:sync-collection xmlns:d=\"DAV:\"><d:sync-token/></d:sync-collection>";
    let (_s, _h, xml) = dav_as(&a, &mate_email, "REPORT", &foreign_coll, None, sync).await;
    assert!(
        !xml.contains("Salary talk") && !xml.contains(&format!("{e}.ics")),
        "sync must not leak members: {xml}"
    );
    let mate_obj = format!("/dav/calendars/{mu}/default/{e}.ics");
    assert_eq!(
        dav_as(&a, &mate_email, "GET", &mate_obj, None, "").await.0,
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        dav_as(&a, &mate_email, "DELETE", &mate_obj, None, "")
            .await
            .0,
        StatusCode::NOT_FOUND
    );
    let (status, ..) = dav_as(
        &a,
        &mate_email,
        "PUT",
        &format!("/dav/calendars/{mu}/cal_personal_{ua}/inject-{mu}.ics"),
        None,
        &ics(
            &format!("inject-{mu}"),
            "Injected",
            "20260610T090000Z",
            "20260610T100000Z",
        ),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::NOT_FOUND,
        "an unshared calendar id is unprobeable"
    );
}

/// A shared calendar at viewer role reads fine and refuses writes with 403 —
/// visible is not editable; raising the grant to editor lets the PUT through.
#[tokio::test]
async fn read_only_shared_calendar_refuses_writes_with_403() {
    let a = harness("caldav-share").await;
    let ua = &a.account_id;

    let mate_email = format!("viewer-{}@example.test", a.tenant);
    let mate = a.ts.create_user(&mate_email).await.unwrap();
    a.identity
        .set_password(&a.tenant, &mate, &mate_email, "s3cret-pw")
        .await
        .unwrap();

    // A owns a team calendar carrying one event, shared read-only.
    let team = a
        .acc
        .create_calendar("Team", Some("#2a9d8f"))
        .await
        .unwrap();
    let e = format!("team-{ua}");
    let team_obj = format!("/dav/calendars/{ua}/{}/{e}.ics", team.as_str());
    let (status, ..) = dav(
        &a,
        "PUT",
        &team_obj,
        None,
        &ics(&e, "Team offsite", "20260610T090000Z", "20260610T100000Z"),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    a.acc
        .grant_calendar(&team, "user", mate.as_str(), "viewer")
        .await
        .unwrap();

    // The viewer's calendar home lists the share, marked read-only.
    let mu = mate.to_string();
    let (_s, _h, xml) = dav_as(
        &a,
        &mate_email,
        "PROPFIND",
        &format!("/dav/calendars/{mu}/"),
        Some("1"),
        "",
    )
    .await;
    assert!(
        xml.contains(&format!("/dav/calendars/{mu}/{}/", team.as_str())),
        "the share is listed: {xml}"
    );
    assert!(
        xml.contains("current-user-privilege-set"),
        "read-only is advertised: {xml}"
    );

    // Reading works; writing is a 403 (never a 500, never misfiled), for both
    // PUT (replace and new-object) and DELETE.
    let mate_obj = format!("/dav/calendars/{mu}/{}/{e}.ics", team.as_str());
    let (status, _h, body) = dav_as(&a, &mate_email, "GET", &mate_obj, None, "").await;
    assert_eq!(status, StatusCode::OK);
    assert!(body.contains("Team offsite"));
    let (status, ..) = dav_as(
        &a,
        &mate_email,
        "PUT",
        &mate_obj,
        None,
        &ics(&e, "Vandalized", "20260610T090000Z", "20260610T100000Z"),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "viewer PUT is refused");
    let (status, ..) = dav_as(&a, &mate_email, "DELETE", &mate_obj, None, "").await;
    assert_eq!(status, StatusCode::FORBIDDEN, "viewer DELETE is refused");
    let (_s, _h, body) = dav(&a, "GET", &team_obj, None, "").await;
    assert!(body.contains("Team offsite"), "the event is intact: {body}");

    // Raised to editor, the same PUT lands.
    a.acc
        .grant_calendar(&team, "user", mate.as_str(), "editor")
        .await
        .unwrap();
    let (status, ..) = dav_as(
        &a,
        &mate_email,
        "PUT",
        &mate_obj,
        None,
        &ics(&e, "Moved offsite", "20260611T090000Z", "20260611T100000Z"),
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT, "editor PUT replaces");
    let (_s, _h, body) = dav(&a, "GET", &team_obj, None, "").await;
    assert!(body.contains("Moved offsite"), "{body}");
}

#[tokio::test]
async fn recurring_series_syncs_zoned_and_time_range_expands() {
    let h = harness("caldav-dst").await;
    let uid = &h.account_id;
    let cal = format!("/dav/calendars/{uid}/default/");

    // A Brussels weekly (09:00 local from Mon 2026-10-19, six instances)
    // crossing the 2026-10-25 end of DST, with the Nov 2 instance cancelled
    // and an extra Thursday added — as a zone-aware client PUTs it.
    let series = format!("dst-{uid}");
    let series_path = format!("{cal}{series}.ics");
    let body = format!(
        "BEGIN:VCALENDAR\r\nVERSION:2.0\r\nPRODID:-//Test//caldav//EN\r\n\
         BEGIN:VEVENT\r\nUID:{series}\r\nDTSTAMP:20260101T000000Z\r\n\
         DTSTART;TZID=Europe/Brussels:20261019T090000\r\n\
         DTEND;TZID=Europe/Brussels:20261019T093000\r\n\
         RRULE:FREQ=WEEKLY;COUNT=6\r\n\
         EXDATE;TZID=Europe/Brussels:20261102T090000\r\n\
         RDATE;TZID=Europe/Brussels:20261022T090000\r\n\
         SUMMARY:Monday review\r\nEND:VEVENT\r\nEND:VCALENDAR\r\n"
    );
    let (status, ..) = dav(&h, "PUT", &series_path, None, &body).await;
    assert_eq!(status, StatusCode::CREATED);

    // GET serves the wall-clock (`;TZID=`) form back — the client's own
    // expansion stays DST-correct — with the RDATE and EXDATE intact.
    let (status, _h2, ics_body) = dav(&h, "GET", &series_path, None, "").await;
    assert_eq!(status, StatusCode::OK);
    assert!(
        ics_body.contains("DTSTART;TZID=Europe/Brussels:20261019T090000"),
        "{ics_body}"
    );
    assert!(ics_body.contains("RRULE:FREQ=WEEKLY;COUNT=6"), "{ics_body}");
    assert!(
        ics_body.contains("RDATE;TZID=Europe/Brussels:20261022T090000"),
        "{ics_body}"
    );
    assert!(
        ics_body.contains("EXDATE;TZID=Europe/Brussels:20261102T090000"),
        "{ics_body}"
    );

    // A bounded series entirely before November: the old "master starts
    // before the window end" shortcut would have kept it; real expansion
    // (the same function the Agenda uses) excludes it.
    let spent = format!("spent-{uid}");
    let spent_path = format!("{cal}{spent}.ics");
    let body = format!(
        "BEGIN:VCALENDAR\r\nVERSION:2.0\r\nPRODID:-//Test//caldav//EN\r\n\
         BEGIN:VEVENT\r\nUID:{spent}\r\nDTSTAMP:20260101T000000Z\r\n\
         DTSTART:20260601T080000Z\r\nDTEND:20260601T083000Z\r\n\
         RRULE:FREQ=DAILY;COUNT=3\r\n\
         SUMMARY:June only\r\nEND:VEVENT\r\nEND:VCALENDAR\r\n"
    );
    let (status, ..) = dav(&h, "PUT", &spent_path, None, &body).await;
    assert_eq!(status, StatusCode::CREATED);

    // November window: the Brussels series still has instances (Nov 9/16/23,
    // 09:00 CET) so it is reported; the June-only series is not.
    let query = "<c:calendar-query xmlns:d=\"DAV:\" xmlns:c=\"urn:ietf:params:xml:ns:caldav\">\
         <c:filter><c:comp-filter name=\"VCALENDAR\"><c:comp-filter name=\"VEVENT\">\
         <c:time-range start=\"20261101T000000Z\" end=\"20261201T000000Z\"/>\
         </c:comp-filter></c:comp-filter></c:filter></c:calendar-query>";
    let (status, _h3, xml) = dav(&h, "REPORT", &cal, Some("1"), query).await;
    assert_eq!(status, StatusCode::MULTI_STATUS);
    assert!(
        xml.contains(&format!("{series}.ics")),
        "the DST-crossing series has November instances: {xml}"
    );
    assert!(
        !xml.contains(&format!("{spent}.ics")),
        "a series spent before the window is narrowed out: {xml}"
    );
}
