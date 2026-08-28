//! JMAP for Sieve (RFC 9661) over the in-process router: script set/get/
//! activate/validate, invalid-script rejection, and cross-account isolation
//! of script CRUD (a second user in the same tenant cannot reach the
//! first's scripts).
#![allow(clippy::unwrap_used, clippy::expect_used)]

use crate::common::*;
use serde_json::{Value, json};

const SIEVE: &str = "urn:ietf:params:jmap:sieve";

/// A single SieveScript method call wrapped in a Request using the sieve
/// capability.
fn sieve_call(method: &str, args: Value) -> Value {
    json!({
        "using": ["urn:ietf:params:jmap:core", SIEVE],
        "methodCalls": [[method, args, "c0"]]
    })
}

fn response(body: &Value) -> &Value {
    &body["methodResponses"][0][1]
}

#[tokio::test]
async fn set_get_activate_and_validate() {
    let h = harness("sieve").await;
    let account = h.account_id.clone();

    // Create a valid script.
    let (status, body) = api(
        &h.app,
        &h.token,
        sieve_call(
            "SieveScript/set",
            json!({
                "accountId": account,
                "create": {
                    "s1": { "name": "main", "content": "require [\"fileinto\"]; fileinto \"INBOX\";" }
                }
            }),
        ),
    )
    .await;
    assert_eq!(status, 200);
    assert!(response(&body)["created"]["s1"].is_object(), "{body}");

    // Get it back with content + isActive false.
    let (_s, body) = api(
        &h.app,
        &h.token,
        sieve_call(
            "SieveScript/get",
            json!({ "accountId": account, "ids": Value::Null }),
        ),
    )
    .await;
    let list = response(&body)["list"].as_array().unwrap();
    assert_eq!(list.len(), 1);
    assert_eq!(list[0]["name"], "main");
    assert_eq!(list[0]["isActive"], false);
    assert!(list[0]["content"].as_str().unwrap().contains("fileinto"));

    // Activate it.
    let (_s, _b) = api(
        &h.app,
        &h.token,
        sieve_call(
            "SieveScript/set",
            json!({ "accountId": account, "onSuccessActivateScript": "main" }),
        ),
    )
    .await;
    let (_s, body) = api(
        &h.app,
        &h.token,
        sieve_call(
            "SieveScript/get",
            json!({ "accountId": account, "ids": Value::Null }),
        ),
    )
    .await;
    assert_eq!(response(&body)["list"][0]["isActive"], true);

    // An invalid script is rejected on create with invalidScript.
    let (_s, body) = api(
        &h.app,
        &h.token,
        sieve_call(
            "SieveScript/set",
            json!({
                "accountId": account,
                "create": { "bad": { "name": "bad", "content": "fileinto \"X\";" } }
            }),
        ),
    )
    .await;
    assert_eq!(
        response(&body)["notCreated"]["bad"]["type"],
        "invalidScript"
    );

    // validate reports the same without storing.
    let (_s, body) = api(
        &h.app,
        &h.token,
        sieve_call(
            "SieveScript/validate",
            json!({ "accountId": account, "content": "fileinto \"X\";" }),
        ),
    )
    .await;
    assert_eq!(response(&body)["isValid"], false);
    assert!(response(&body)["errorDescription"].is_string());
}

#[tokio::test]
async fn create_and_activate_in_one_call_resolves_creation_id() {
    // RFC 9661 §2.5: onSuccessActivateScript may reference a `#creationId`
    // created in the same /set.
    let h = harness("sieve-activate").await;
    let (_s, body) = api(
        &h.app,
        &h.token,
        sieve_call(
            "SieveScript/set",
            json!({
                "accountId": h.account_id,
                "create": { "new": { "name": "onboarding", "content": "keep;" } },
                "onSuccessActivateScript": "#new"
            }),
        ),
    )
    .await;
    assert!(response(&body)["created"]["new"].is_object(), "{body}");
    // The created script is now the active one.
    let (_s, body) = api(
        &h.app,
        &h.token,
        sieve_call(
            "SieveScript/get",
            json!({ "accountId": h.account_id, "ids": Value::Null }),
        ),
    )
    .await;
    let list = response(&body)["list"].as_array().unwrap();
    assert_eq!(list.len(), 1);
    assert_eq!(list[0]["isActive"], true, "created-then-activated: {body}");
}

#[tokio::test]
async fn scripts_are_isolated_across_accounts_in_one_tenant() {
    let h = harness("sieve-iso").await;
    // User A (the harness account) creates a script.
    api(
        &h.app,
        &h.token,
        sieve_call(
            "SieveScript/set",
            json!({
                "accountId": h.account_id,
                "create": { "s": { "name": "secret", "content": "keep;" } }
            }),
        ),
    )
    .await;

    // A second user B in the SAME tenant with its own token.
    let email_b = format!("b-{}@example.test", h.user);
    let ub = h.ts.create_user(&email_b).await.unwrap();
    h.identity
        .set_password(h.ts.tenant(), &ub, &email_b, "pw-b")
        .await
        .unwrap();
    let token_b = h
        .identity
        .password_login(&email_b, "pw-b", None)
        .await
        .unwrap()
        .unwrap()
        .0
        .reveal()
        .to_owned();
    let account_b = ub.to_string();

    // B's SieveScript/get returns nothing — A's script is invisible.
    let (_s, body) = api(
        &h.app,
        &token_b,
        sieve_call(
            "SieveScript/get",
            json!({ "accountId": account_b, "ids": Value::Null }),
        ),
    )
    .await;
    assert!(
        response(&body)["list"].as_array().unwrap().is_empty(),
        "B must not see A's scripts: {body}"
    );

    // B addressing A's accountId is rejected (accountNotFound), never data.
    let (_s, body) = api(
        &h.app,
        &token_b,
        sieve_call(
            "SieveScript/get",
            json!({ "accountId": h.account_id, "ids": Value::Null }),
        ),
    )
    .await;
    assert_eq!(body["methodResponses"][0][0], "error");
    assert_eq!(response(&body)["type"], "accountNotFound");
}
