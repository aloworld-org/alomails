//! End-to-end address book: Contact/set (create/update/destroy) and
//! Contact/get through the real JMAP router against Postgres, plus the
//! mandatory tenant-isolation test — one account can never see or touch
//! another's contacts — and the compose-autocomplete tie-in.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use crate::common;

use crate::common::{Harness, api, harness};
use serde_json::{Value, json};

const CONTACTS_USING: [&str; 2] = ["urn:ietf:params:jmap:core", "urn:ietf:params:jmap:contacts"];

async fn set(h: &Harness, method_args: Value) -> Value {
    let body = json!({
        "using": CONTACTS_USING,
        "methodCalls": [["Contact/set", method_args, "0"]],
    });
    let (status, resp) = api(&h.app, &h.token, body).await;
    assert_eq!(status, 200, "{resp}");
    resp["methodResponses"][0][1].clone()
}

async fn get_all(h: &Harness) -> Vec<Value> {
    let body = json!({
        "using": CONTACTS_USING,
        "methodCalls": [["Contact/get", { "accountId": h.account_id, "ids": Value::Null }, "0"]],
    });
    let (status, resp) = api(&h.app, &h.token, body).await;
    assert_eq!(status, 200, "{resp}");
    resp["methodResponses"][0][1]["list"]
        .as_array()
        .cloned()
        .unwrap_or_default()
}

#[tokio::test]
async fn create_get_update_destroy() {
    let h = harness("contacts-crud").await;

    // Create from structured fields; the display name derives from first+last.
    let created = set(
        &h,
        json!({
            "accountId": h.account_id,
            "create": { "c1": {
                "firstName": "Alice", "lastName": "Martin",
                "emails": [{ "kind": "work", "value": "alice@example.eu" }],
                "phones": [{ "value": "+33123" }],
                "organization": "Example SARL"
            }}
        }),
    )
    .await;
    let id = created["created"]["c1"]["id"]
        .as_str()
        .expect("new id")
        .to_owned();

    let list = get_all(&h).await;
    assert_eq!(list.len(), 1);
    assert_eq!(list[0]["name"], "Alice Martin");
    assert_eq!(list[0]["emails"][0]["value"], "alice@example.eu");
    assert_eq!(list[0]["emails"][0]["kind"], "work");
    assert_eq!(list[0]["organization"], "Example SARL");

    // Partial update: change only the org; name and emails must persist.
    let updated = set(
        &h,
        json!({ "accountId": h.account_id, "update": { &id: { "organization": "New Co" } } }),
    )
    .await;
    assert!(updated["updated"].get(&id).is_some(), "{updated}");
    let list = get_all(&h).await;
    assert_eq!(list[0]["organization"], "New Co");
    assert_eq!(
        list[0]["name"], "Alice Martin",
        "name preserved on partial update"
    );
    assert_eq!(list[0]["emails"][0]["value"], "alice@example.eu");

    // Destroy.
    let destroyed = set(&h, json!({ "accountId": h.account_id, "destroy": [&id] })).await;
    assert_eq!(destroyed["destroyed"][0], json!(id));
    assert!(get_all(&h).await.is_empty());
}

#[tokio::test]
async fn create_requires_a_derivable_name() {
    let h = harness("contacts-name").await;
    // Neither name nor first/last nor email → invalidProperties, not a 500.
    let resp = set(
        &h,
        json!({ "accountId": h.account_id, "create": { "c1": { "notes": "just a note" } } }),
    )
    .await;
    assert_eq!(resp["notCreated"]["c1"]["type"], "invalidProperties");
    assert!(resp["created"].as_object().unwrap().is_empty());

    // An email alone is enough — the address becomes the display name.
    let resp = set(
        &h,
        json!({ "accountId": h.account_id, "create": { "c2": {
            "emails": [{ "value": "only@address.eu" }]
        }}}),
    )
    .await;
    assert!(resp["created"].get("c2").is_some(), "{resp}");
    assert_eq!(get_all(&h).await[0]["name"], "only@address.eu");
}

#[tokio::test]
async fn saved_contacts_surface_in_compose_autocomplete() {
    let h = harness("contacts-autocomplete").await;
    set(
        &h,
        json!({ "accountId": h.account_id, "create": { "c1": {
            "firstName": "Grace", "lastName": "Hopper",
            "emails": [{ "value": "grace@navy.mil" }]
        }}}),
    )
    .await;
    // GET /contacts merges saved contacts into the mined suggestions.
    let (status, body) = common::get(&h.app, &h.token, "/contacts").await;
    assert_eq!(status, 200, "{body}");
    let list = body["contacts"].as_array().cloned().unwrap_or_default();
    let hit = list
        .iter()
        .find(|c| c["email"] == "grace@navy.mil")
        .expect("saved contact appears in autocomplete");
    assert_eq!(hit["name"], "Grace Hopper");
}

#[tokio::test]
async fn contacts_are_tenant_isolated() {
    // Two accounts in different tenants. One creates a contact; the other
    // must neither see it in Contact/get nor be able to update/destroy it.
    let a = harness("contacts-iso-a").await;
    let b = harness("contacts-iso-b").await;

    let created = set(
        &a,
        json!({ "accountId": a.account_id, "create": { "c1": {
            "firstName": "Secret", "lastName": "Contact",
            "emails": [{ "value": "secret@a.example" }]
        }}}),
    )
    .await;
    let id = created["created"]["c1"]["id"].as_str().unwrap().to_owned();

    // B's Contact/get returns nothing of A's, even asking for the id.
    let body = json!({
        "using": CONTACTS_USING,
        "methodCalls": [["Contact/get", { "accountId": b.account_id, "ids": [&id] }, "0"]],
    });
    let (_s, resp) = api(&b.app, &b.token, body).await;
    let r = &resp["methodResponses"][0][1];
    assert!(
        r["list"].as_array().unwrap().is_empty(),
        "no cross-tenant read: {r}"
    );
    assert_eq!(r["notFound"][0], json!(id), "the id is notFound for B");
    assert!(get_all(&b).await.is_empty(), "B's address book stays empty");

    // B's update/destroy of A's id are clean denials (notFound), not data
    // and not a 500 — and A's contact is untouched afterwards.
    let upd = set(
        &b,
        json!({ "accountId": b.account_id, "update": { &id: { "notes": "tampered" } } }),
    )
    .await;
    assert_eq!(upd["notUpdated"][&id]["type"], "notFound");
    let del = set(&b, json!({ "accountId": b.account_id, "destroy": [&id] })).await;
    assert!(
        del["notDestroyed"].get(&id).is_some(),
        "cross-tenant destroy denied: {del}"
    );

    let a_list = get_all(&a).await;
    assert_eq!(a_list.len(), 1, "A's contact survived B's tampering");
    assert_eq!(a_list[0]["notes"], Value::Null);
}
