//! `SearchSnippet/get` (RFC 8621 §5.1): the matched search terms are
//! highlighted (`<mark>`) in the subject and preview of each requested email.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use crate::common::{api, harness};
use serde_json::{Value, json};

fn call(account_id: &str, method: &str, mut args: Value) -> Value {
    args["accountId"] = json!(account_id);
    json!({
        "using": ["urn:ietf:params:jmap:core", "urn:ietf:params:jmap:mail"],
        "methodCalls": [[method, args, "c"]],
    })
}

#[tokio::test]
async fn search_snippet_highlights_matches() {
    let h = harness("snippet").await;
    let acc = h.user.to_string();
    let own = h.store.for_account(h.tenant.clone(), h.user.clone());
    own.deliver(b"From: a@x\r\nSubject: Project Falcon\r\n\r\nthe falcon has landed\r\n")
        .await
        .unwrap();

    // Find the delivered message.
    let (_s, body) = api(
        &h.app,
        &h.token,
        call(
            &acc,
            "Email/query",
            json!({ "filter": { "text": "falcon" } }),
        ),
    )
    .await;
    let id = body["methodResponses"][0][1]["ids"][0]
        .as_str()
        .unwrap()
        .to_owned();

    // Snippet it with the same query.
    let (_s, body) = api(
        &h.app,
        &h.token,
        call(
            &acc,
            "SearchSnippet/get",
            json!({ "filter": { "text": "falcon" }, "emailIds": [id] }),
        ),
    )
    .await;
    let snip = &body["methodResponses"][0][1]["list"][0];
    assert_eq!(
        snip["subject"],
        json!("Project <mark>Falcon</mark>"),
        "{body}"
    );
    assert!(
        snip["preview"]
            .as_str()
            .unwrap()
            .contains("<mark>falcon</mark>"),
        "preview highlights the match: {body}",
    );
}
