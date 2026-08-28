//! `Identity/get` (RFC 8621 §6.1): a standard JMAP client discovers the
//! addresses it may send from — the user's canonical address plus each alias —
//! before it can submit. Read-only (provisioned identities).
#![allow(clippy::unwrap_used, clippy::expect_used)]

use crate::common::{api, harness};
use serde_json::{Value, json};

fn call(account_id: &str, method: &str, mut args: Value) -> Value {
    args["accountId"] = json!(account_id);
    json!({
        "using": [
            "urn:ietf:params:jmap:core",
            "urn:ietf:params:jmap:mail",
            "urn:ietf:params:jmap:submission"
        ],
        "methodCalls": [[method, args, "c"]],
    })
}

#[tokio::test]
async fn identity_get_lists_send_addresses() {
    let h = harness("identity").await;
    let alias = format!("alias-{}@example.test", h.tenant);
    h.ts.add_alias(&h.user, &alias).await.unwrap();
    let acct = h.user.to_string();

    let (_s, body) = api(
        &h.app,
        &h.token,
        call(&acct, "Identity/get", json!({ "ids": null })),
    )
    .await;
    assert_eq!(
        body["methodResponses"][0][0],
        json!("Identity/get"),
        "{body}"
    );
    let list = body["methodResponses"][0][1]["list"].as_array().unwrap();
    // Canonical address + the alias, one identity each.
    assert!(
        list.len() >= 2,
        "expected canonical + alias identities: {body}"
    );
    for ident in list {
        assert!(ident["id"].as_str().is_some(), "identity has an id: {body}");
        assert!(
            ident["email"].as_str().is_some(),
            "identity has an email: {body}"
        );
        assert_eq!(
            ident["mayDelete"],
            json!(false),
            "provisioned identity: {body}"
        );
    }
    // Aliases are stored lowercased.
    let alias_lc = alias.to_lowercase();
    let emails: Vec<&str> = list.iter().filter_map(|i| i["email"].as_str()).collect();
    assert!(
        emails.iter().any(|e| e.eq_ignore_ascii_case(&alias_lc)),
        "alias identity present: {body}"
    );

    // Filtering by id returns exactly that identity; an unknown id is notFound.
    let first_id = list[0]["id"].as_str().unwrap().to_owned();
    let (_s, body) = api(
        &h.app,
        &h.token,
        call(
            &acct,
            "Identity/get",
            json!({ "ids": [first_id, "deadbeefdeadbeef"] }),
        ),
    )
    .await;
    let list = body["methodResponses"][0][1]["list"].as_array().unwrap();
    assert_eq!(list.len(), 1, "one identity for the known id: {body}");
    assert_eq!(list[0]["id"].as_str().unwrap(), first_id);
    assert_eq!(
        body["methodResponses"][0][1]["notFound"][0],
        json!("deadbeefdeadbeef")
    );
}
