//! Category (alo extension) over the wire: the catalog CRUD through
//! Category/get + Category/set, tagging a message with the returned keyword via
//! the standard Email/set, filtering by it with Email/query hasKeyword, and the
//! guarantee that destroying a category strips its tag from messages.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use crate::common::{api, harness};
use axum::http::StatusCode;
use serde_json::{Map, Value, json};

const USING: [&str; 3] = [
    "urn:ietf:params:jmap:core",
    "urn:ietf:params:jmap:mail",
    "urn:alo:params:jmap:categories",
];

/// One method call with the categories capability in `using`.
fn one(account: &str, method: &str, mut args: Value) -> Value {
    args["accountId"] = json!(account);
    json!({ "using": USING, "methodCalls": [[method, args, "c0"]] })
}

/// A single-entry JSON object with a dynamic key (json! only takes literal keys).
fn obj1(key: String, value: Value) -> Value {
    let mut m = Map::new();
    m.insert(key, value);
    Value::Object(m)
}

#[tokio::test]
async fn full_category_lifecycle_over_the_wire() {
    let h = harness("cat").await;
    let acc = h.account_id.as_str();

    // Empty catalog to begin with.
    let (status, body) = api(&h.app, &h.token, one(acc, "Category/get", json!({}))).await;
    assert_eq!(status, StatusCode::OK);
    let resp = &body["methodResponses"][0];
    assert_eq!(resp[0], json!("Category/get"));
    assert_eq!(resp[1]["list"].as_array().unwrap().len(), 0);

    // Create a category; the response hands back its id and its keyword.
    let (_s, body) = api(
        &h.app,
        &h.token,
        one(
            acc,
            "Category/set",
            json!({ "create": { "c": { "name": "Work", "color": "#3f7cac" } } }),
        ),
    )
    .await;
    let created = &body["methodResponses"][0][1]["created"]["c"];
    let cat_id = created["id"].as_str().expect("created id").to_owned();
    let keyword = created["keyword"].as_str().expect("keyword").to_owned();
    assert_eq!(keyword, format!("$category_{cat_id}"));

    // It shows up in Category/get with its color.
    let (_s, body) = api(&h.app, &h.token, one(acc, "Category/get", json!({}))).await;
    let list = body["methodResponses"][0][1]["list"].as_array().unwrap();
    assert_eq!(list.len(), 1);
    assert_eq!(list[0]["name"], json!("Work"));
    assert_eq!(list[0]["color"], json!("#3f7cac"));

    // Tag a delivered message with the category keyword via the standard path.
    let mid = h
        .acc
        .deliver(b"From: a@x\r\nSubject: invoice\r\n\r\nbody\r\n")
        .await
        .unwrap();
    let tag_patch = obj1(format!("keywords/{keyword}"), json!(true));
    let update = obj1(mid.to_string(), tag_patch);
    let (_s, body) = api(
        &h.app,
        &h.token,
        one(acc, "Email/set", json!({ "update": update })),
    )
    .await;
    assert!(
        body["methodResponses"][0][1]["updated"]
            .get(mid.to_string().as_str())
            .is_some(),
        "the tag update succeeded: {body}",
    );

    // Email/query hasKeyword finds exactly that message.
    let (_s, body) = api(
        &h.app,
        &h.token,
        one(
            acc,
            "Email/query",
            json!({ "filter": { "hasKeyword": keyword } }),
        ),
    )
    .await;
    let ids = body["methodResponses"][0][1]["ids"].as_array().unwrap();
    assert_eq!(ids.len(), 1);
    assert_eq!(ids[0], json!(mid.to_string()));

    // Destroying the category strips the tag: the message no longer carries it,
    // and the hasKeyword query goes empty.
    let (_s, body) = api(
        &h.app,
        &h.token,
        one(acc, "Category/set", json!({ "destroy": [cat_id] })),
    )
    .await;
    assert_eq!(body["methodResponses"][0][1]["destroyed"], json!([cat_id]),);

    let (_s, body) = api(
        &h.app,
        &h.token,
        one(
            acc,
            "Email/get",
            json!({ "ids": [mid.to_string()], "properties": ["keywords"] }),
        ),
    )
    .await;
    let kws = &body["methodResponses"][0][1]["list"][0]["keywords"];
    assert!(
        kws.get(keyword.as_str()).is_none(),
        "the dangling category keyword must be gone: {kws}",
    );

    let (_s, body) = api(
        &h.app,
        &h.token,
        one(
            acc,
            "Email/query",
            json!({ "filter": { "hasKeyword": keyword } }),
        ),
    )
    .await;
    assert_eq!(
        body["methodResponses"][0][1]["ids"]
            .as_array()
            .unwrap()
            .len(),
        0,
    );
}

#[tokio::test]
async fn rejects_bad_color_and_duplicate_name() {
    let h = harness("catbad").await;
    let acc = h.account_id.as_str();

    // A non-hex color is invalidProperties (not created).
    let (_s, body) = api(
        &h.app,
        &h.token,
        one(
            acc,
            "Category/set",
            json!({ "create": { "c": { "name": "X", "color": "red" } } }),
        ),
    )
    .await;
    assert_eq!(
        body["methodResponses"][0][1]["notCreated"]["c"]["type"],
        json!("invalidProperties"),
    );

    // First "Dup" succeeds; a second with the same name is a conflict.
    let make = |cid: &str| {
        let create = obj1(cid.to_owned(), json!({ "name": "Dup" }));
        one(acc, "Category/set", json!({ "create": create }))
    };
    let (_s, body) = api(&h.app, &h.token, make("a")).await;
    assert!(body["methodResponses"][0][1]["created"]["a"].is_object());
    let (_s, body) = api(&h.app, &h.token, make("b")).await;
    assert!(
        body["methodResponses"][0][1]["notCreated"]["b"].is_object(),
        "a duplicate name is rejected: {body}",
    );
}
