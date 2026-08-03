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
