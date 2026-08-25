//! `VacationResponse/get`+`/set` (RFC 8621 §8): the singleton auto-reply,
//! backed by the account's out-of-office. A standard client can read and toggle
//! it; create/destroy are refused (it is a singleton).
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use common::{api, harness};
use serde_json::{Value, json};

fn call(account_id: &str, method: &str, mut args: Value) -> Value {
    args["accountId"] = json!(account_id);
    json!({
        "using": ["urn:ietf:params:jmap:core", "urn:ietf:params:jmap:mail", "urn:ietf:params:jmap:vacationresponse"],
        "methodCalls": [[method, args, "c"]],
    })
}

#[tokio::test]
async fn vacation_response_round_trips() {
    let h = harness("vacation").await;
    let acct = h.user.to_string();

    // Initially the singleton exists and is disabled.
    let (_s, body) = api(
        &h.app,
        &h.token,
        call(&acct, "VacationResponse/get", json!({ "ids": null })),
    )
    .await;
    let obj = &body["methodResponses"][0][1]["list"][0];
    assert_eq!(obj["id"], json!("singleton"), "{body}");
    assert_eq!(obj["isEnabled"], json!(false), "{body}");

    // Enable it with a subject + body.
    let (_s, body) = api(
        &h.app,
        &h.token,
        call(
            &acct,
            "VacationResponse/set",
            json!({ "update": { "singleton": {
                "isEnabled": true, "subject": "Away", "textBody": "Back Monday"
            }}}),
        ),
    )
    .await;
    assert!(
        body["methodResponses"][0][1]["updated"]
            .get("singleton")
            .is_some(),
        "singleton updated: {body}",
    );

    // Read it back.
    let (_s, body) = api(
        &h.app,
        &h.token,
        call(&acct, "VacationResponse/get", json!({ "ids": null })),
    )
    .await;
    let obj = &body["methodResponses"][0][1]["list"][0];
    assert_eq!(obj["isEnabled"], json!(true), "{body}");
    assert_eq!(obj["subject"], json!("Away"), "{body}");
    assert_eq!(obj["textBody"], json!("Back Monday"), "{body}");

    // Enabling with no message is rejected.
    let (_s, body) = api(
        &h.app,
        &h.token,
        call(
            &acct,
            "VacationResponse/set",
            json!({ "update": { "singleton": { "isEnabled": true, "textBody": "" }}}),
        ),
    )
    .await;
    assert_eq!(
        body["methodResponses"][0][1]["notUpdated"]["singleton"]["type"],
        json!("invalidProperties"),
        "{body}",
    );

    // Creating a second one is refused (singleton).
    let (_s, body) = api(
        &h.app,
        &h.token,
        call(
            &acct,
            "VacationResponse/set",
            json!({ "create": { "x": { "isEnabled": false } }}),
        ),
    )
    .await;
    assert_eq!(
        body["methodResponses"][0][1]["notCreated"]["x"]["type"],
        json!("forbidden"),
        "{body}",
    );

    // Disable it again.
    let (_s, body) = api(
        &h.app,
        &h.token,
        call(
            &acct,
            "VacationResponse/set",
            json!({ "update": { "singleton": { "isEnabled": false }}}),
        ),
    )
    .await;
    assert!(
        body["methodResponses"][0][1]["updated"]
            .get("singleton")
            .is_some(),
        "{body}"
    );
}

#[tokio::test]
async fn the_dates_a_client_schedules_come_back_unchanged() {
    // We advertise `urn:ietf:params:jmap:vacationresponse`, and both dates were
    // accepted and then reported as null whatever the client had sent. A client
    // that scheduled a holiday was told, in the response to its own request,
    // that its dates had not been stored.
    let h = harness("vacation-dates").await;
    let acct = h.user.to_string();

    let (_s, body) = api(
        &h.app,
        &h.token,
        call(
            &acct,
            "VacationResponse/set",
            json!({ "update": { "singleton": {
                "isEnabled": true,
                "subject": "Away",
                "textBody": "Back on the 15th",
                "fromDate": "2026-09-01T00:00:00Z",
                "toDate": "2026-09-15T00:00:00Z",
            }}}),
        ),
    )
    .await;
    assert!(
        body["methodResponses"][0][1]["updated"]
            .get("singleton")
            .is_some(),
        "{body}",
    );

    let (_s, body) = api(
        &h.app,
        &h.token,
        call(&acct, "VacationResponse/get", json!({ "ids": null })),
    )
    .await;
    let obj = &body["methodResponses"][0][1]["list"][0];
    assert_eq!(obj["fromDate"], json!("2026-09-01T00:00:00Z"), "{body}");
    assert_eq!(obj["toDate"], json!("2026-09-15T00:00:00Z"), "{body}");
}

#[tokio::test]
async fn a_patch_that_names_no_date_leaves_the_holiday_alone() {
    // RFC 8620 §5.3 patches only what it names. A client toggling the subject
    // must not silently cancel the window somebody set from another device.
    let h = harness("vacation-patch").await;
    let acct = h.user.to_string();

    let (_s, _b) = api(
        &h.app,
        &h.token,
        call(
            &acct,
            "VacationResponse/set",
            json!({ "update": { "singleton": {
                "isEnabled": true,
                "textBody": "Away",
                "fromDate": "2026-09-01T00:00:00Z",
                "toDate": "2026-09-15T00:00:00Z",
            }}}),
        ),
    )
    .await;

    let (_s, _b) = api(
        &h.app,
        &h.token,
        call(
            &acct,
            "VacationResponse/set",
            json!({ "update": { "singleton": { "subject": "On holiday" }}}),
        ),
    )
    .await;

    let (_s, body) = api(
        &h.app,
        &h.token,
        call(&acct, "VacationResponse/get", json!({ "ids": null })),
    )
    .await;
    let obj = &body["methodResponses"][0][1]["list"][0];
    assert_eq!(obj["subject"], json!("On holiday"), "{body}");
    assert_eq!(obj["fromDate"], json!("2026-09-01T00:00:00Z"), "{body}");
    assert_eq!(obj["toDate"], json!("2026-09-15T00:00:00Z"), "{body}");
}

#[tokio::test]
async fn a_date_set_to_null_clears_that_bound() {
    // The other half of the patch rule: naming a date as null is how RFC 8621
    // §8 says "no end", and must be told apart from not naming it at all.
    let h = harness("vacation-null").await;
    let acct = h.user.to_string();

    let (_s, _b) = api(
        &h.app,
        &h.token,
        call(
            &acct,
            "VacationResponse/set",
            json!({ "update": { "singleton": {
                "isEnabled": true,
                "textBody": "Away",
                "fromDate": "2026-09-01T00:00:00Z",
                "toDate": "2026-09-15T00:00:00Z",
            }}}),
        ),
    )
    .await;

    let (_s, _b) = api(
        &h.app,
        &h.token,
        call(
            &acct,
            "VacationResponse/set",
            json!({ "update": { "singleton": { "toDate": Value::Null }}}),
        ),
    )
    .await;

    let (_s, body) = api(
        &h.app,
        &h.token,
        call(&acct, "VacationResponse/get", json!({ "ids": null })),
    )
    .await;
    let obj = &body["methodResponses"][0][1]["list"][0];
    assert_eq!(obj["fromDate"], json!("2026-09-01T00:00:00Z"), "{body}");
    assert_eq!(obj["toDate"], Value::Null, "the end was cleared: {body}");
}

#[tokio::test]
async fn a_window_that_ends_before_it_starts_is_refused() {
    // Stored, it would never fire, and read to whoever set it exactly like the
    // feature being broken.
    let h = harness("vacation-backwards").await;
    let acct = h.user.to_string();

    let (_s, body) = api(
        &h.app,
        &h.token,
        call(
            &acct,
            "VacationResponse/set",
            json!({ "update": { "singleton": {
                "isEnabled": true,
                "textBody": "Away",
                "fromDate": "2026-09-15T00:00:00Z",
                "toDate": "2026-09-01T00:00:00Z",
            }}}),
        ),
    )
    .await;
    assert_eq!(
        body["methodResponses"][0][1]["notUpdated"]["singleton"]["type"],
        json!("invalidProperties"),
        "{body}",
    );
}

#[tokio::test]
async fn a_date_that_is_not_a_date_is_refused_rather_than_ignored() {
    // Quietly dropping an unparseable date is how a client ends up believing a
    // holiday is scheduled when nothing is.
    let h = harness("vacation-garbage").await;
    let acct = h.user.to_string();

    let (_s, body) = api(
        &h.app,
        &h.token,
        call(
            &acct,
            "VacationResponse/set",
            json!({ "update": { "singleton": {
                "isEnabled": true, "textBody": "Away", "fromDate": "next tuesday"
            }}}),
        ),
    )
    .await;
    assert_eq!(
        body["methodResponses"][0][1]["notUpdated"]["singleton"]["type"],
        json!("invalidProperties"),
        "{body}",
    );
}
