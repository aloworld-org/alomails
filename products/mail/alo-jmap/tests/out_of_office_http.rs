//! `GET /settings/mail` and `POST /settings/out-of-office` — the pair the web
//! settings screen uses, and the one place where a stored instant becomes a day
//! on a form and back again.
//!
//! The conversion is the part worth a test. Inside, the end of the window is
//! exclusive: the first moment of the day you are back, so a message arriving
//! that morning reaches a person rather than an auto-reply. On the form it is
//! the last day away. Those are one day apart, in a direction that is easy to
//! get backwards, and getting it backwards is invisible until somebody's date
//! comes back a day out or replies for a day too long.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use crate::common;

use crate::common::{get, harness, post_raw};
use serde_json::{Value, json};

#[tokio::test]
async fn the_days_typed_on_the_form_come_back_as_the_days_typed() {
    let h = harness("ooo-http").await;

    let (status, _b) = post_raw(
        &h.app,
        &h.token,
        "/settings/out-of-office",
        &json!({
            "enabled": true,
            "subject": "Away",
            "message": "Back on the 15th",
            "from": "2026-09-01",
            "to": "2026-09-15",
        })
        .to_string(),
    )
    .await;
    assert!(status.is_success(), "saved: {status}");

    let (status, body) = get(&h.app, &h.token, "/settings/mail").await;
    assert!(status.is_success());
    assert_eq!(body["outOfOffice"]["from"], json!("2026-09-01"), "{body}");
    assert_eq!(
        body["outOfOffice"]["to"],
        json!("2026-09-15"),
        "the last day away, not the day after it: {body}",
    );
}

#[tokio::test]
async fn the_last_day_away_is_covered_to_its_end() {
    // The reason the stored end is exclusive rather than the same midnight:
    // "away until the 15th" has to still reply late on the 15th, and stop by
    // the morning of the 16th.
    let h = harness("ooo-http-end").await;

    let (status, _b) = post_raw(
        &h.app,
        &h.token,
        "/settings/out-of-office",
        &json!({
            "enabled": true,
            "message": "Away",
            "from": "2026-09-01",
            "to": "2026-09-15",
        })
        .to_string(),
    )
    .await;
    assert!(status.is_success());

    // Read through JMAP, which reports the stored instants rather than days.
    let (_s, body) = common::api(
        &h.app,
        &h.token,
        json!({
            "using": ["urn:ietf:params:jmap:core", "urn:ietf:params:jmap:vacationresponse"],
            "methodCalls": [["VacationResponse/get",
                { "accountId": h.user.to_string(), "ids": null }, "c"]],
        }),
    )
    .await;
    let obj = &body["methodResponses"][0][1]["list"][0];
    assert_eq!(obj["fromDate"], json!("2026-09-01T00:00:00Z"), "{body}");
    assert_eq!(
        obj["toDate"],
        json!("2026-09-16T00:00:00Z"),
        "the end is the first moment of the day you are back: {body}",
    );
}

#[tokio::test]
async fn no_dates_leaves_the_window_open_at_both_ends() {
    // What every account had before scheduling existed: on now, until switched
    // off. It has to stay reachable from the same form.
    let h = harness("ooo-http-open").await;

    let (status, _b) = post_raw(
        &h.app,
        &h.token,
        "/settings/out-of-office",
        &json!({ "enabled": true, "message": "Away", "from": null, "to": null }).to_string(),
    )
    .await;
    assert!(status.is_success());

    let (_s, body) = get(&h.app, &h.token, "/settings/mail").await;
    assert_eq!(body["outOfOffice"]["enabled"], json!(true), "{body}");
    assert_eq!(body["outOfOffice"]["from"], Value::Null, "{body}");
    assert_eq!(body["outOfOffice"]["to"], Value::Null, "{body}");
}

/// Sends one message to the harness account and says whether it auto-replied.
///
/// This goes through the same `deliver_sieve` inbound mail does, against the
/// script the settings route just installed — the only way to prove the window
/// the screen saved is the window delivery actually honours.
async fn replies_to_a_message(h: &common::Harness) -> bool {
    let raw = format!(
        "From: bob@ext.test\r\nTo: {}\r\nSubject: hi\r\n\r\nq\r\n",
        h.email
    );
    h.acc
        .deliver_sieve(raw.as_bytes(), Some("bob@ext.test"), &h.email)
        .await
        .unwrap()
        .outbound
        .iter()
        .any(|a| matches!(a, alo_store::OutboundAction::Vacation { .. }))
}

#[tokio::test]
async fn a_holiday_saved_here_is_the_holiday_delivery_honours() {
    // The settings route and the store build the managed Sieve script in two
    // different places. When only one of them marked the reply as ours, the
    // window was stored and displayed perfectly and gated nothing: the screen
    // said "away 1–15 September" and the reply answered every day of the year.
    // Saving through the route and then delivering a message is what tells
    // those two states apart.
    let h = harness("ooo-http-gate").await;
    let away = |from: &str, to: &str| {
        json!({ "enabled": true, "message": "Away", "from": from, "to": to }).to_string()
    };

    let (status, _b) = post_raw(
        &h.app,
        &h.token,
        "/settings/out-of-office",
        &away("2036-09-01", "2036-09-15"),
    )
    .await;
    assert!(status.is_success());
    assert!(
        !replies_to_a_message(&h).await,
        "a holiday ten years out must not answer today",
    );

    // Now one that is running.
    let today = time::OffsetDateTime::now_utc().date();
    let (status, _b) = post_raw(
        &h.app,
        &h.token,
        "/settings/out-of-office",
        &away(
            &today.previous_day().unwrap().to_string(),
            &today.next_day().unwrap().to_string(),
        ),
    )
    .await;
    assert!(status.is_success());
    assert!(
        replies_to_a_message(&h).await,
        "inside the window it answers",
    );
}

#[tokio::test]
async fn an_end_before_the_start_is_refused() {
    let h = harness("ooo-http-backwards").await;

    let (status, _b) = post_raw(
        &h.app,
        &h.token,
        "/settings/out-of-office",
        &json!({
            "enabled": true,
            "message": "Away",
            "from": "2026-09-15",
            "to": "2026-09-01",
        })
        .to_string(),
    )
    .await;
    assert_eq!(status.as_u16(), 400, "refused as bad input");
}

#[tokio::test]
async fn a_date_that_is_not_a_date_is_refused_rather_than_ignored() {
    // Dropping it silently is how somebody ends up believing a holiday is
    // scheduled when nothing is.
    let h = harness("ooo-http-garbage").await;

    let (status, _b) = post_raw(
        &h.app,
        &h.token,
        "/settings/out-of-office",
        &json!({ "enabled": true, "message": "Away", "from": "01/09/2026" }).to_string(),
    )
    .await;
    assert_eq!(status.as_u16(), 400, "refused: {status}");
}
