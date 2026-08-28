//! `Quota/get` (RFC 9425): surfaces the tenant's mail storage cap and usage.
//! A tenant with no cap reports no quota object; a capped tenant reports used +
//! hardLimit octets.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use crate::common::{api, harness};
use serde_json::{Value, json};

fn call(account_id: &str, method: &str, mut args: Value) -> Value {
    args["accountId"] = json!(account_id);
    json!({
        "using": ["urn:ietf:params:jmap:core", "urn:ietf:params:jmap:quota"],
        "methodCalls": [[method, args, "c"]],
    })
}

#[tokio::test]
async fn quota_get_reports_tenant_storage() {
    let h = harness("quota").await;
    let acc = h.user.to_string();

    // No cap by default (unlimited) → no quota object to report.
    let (_s, body) = api(
        &h.app,
        &h.token,
        call(&acc, "Quota/get", json!({ "ids": null })),
    )
    .await;
    assert_eq!(
        body["methodResponses"][0][1]["list"]
            .as_array()
            .unwrap()
            .len(),
        0,
        "unlimited tenant reports no quota: {body}",
    );

    // Give the tenant a cap and store some bytes.
    h.store
        .set_tenant_quota(&h.tenant, Some(10_000_000))
        .await
        .unwrap();
    let own = h.store.for_account(h.tenant.clone(), h.user.clone());
    own.deliver(b"From: a@x\r\nSubject: s\r\n\r\nbody bytes here\r\n")
        .await
        .unwrap();

    let (_s, body) = api(
        &h.app,
        &h.token,
        call(&acc, "Quota/get", json!({ "ids": null })),
    )
    .await;
    let q = &body["methodResponses"][0][1]["list"][0];
    assert_eq!(q["id"], json!("octets"), "{body}");
    assert_eq!(q["resourceType"], json!("octets"), "{body}");
    assert_eq!(q["hardLimit"], json!(10_000_000), "{body}");
    assert!(
        q["used"].as_i64().unwrap() > 0,
        "used bytes are accounted: {body}"
    );

    // An unknown quota id is notFound.
    let (_s, body) = api(
        &h.app,
        &h.token,
        call(&acc, "Quota/get", json!({ "ids": ["nope"] })),
    )
    .await;
    assert_eq!(
        body["methodResponses"][0][1]["notFound"][0],
        json!("nope"),
        "{body}"
    );
    assert!(
        body["methodResponses"][0][1]["list"]
            .as_array()
            .unwrap()
            .is_empty(),
        "{body}"
    );
}
