//! Mailbox delegation over the wire (ADR 0017): a delegate operates on the
//! owner's account only with a grant (else accountNotFound, no oracle); a
//! read-only delegate can read but not mutate; a manage delegate can mutate but
//! not send without a send grant; the session reflects the level; and a user
//! can share their own mailbox self-service.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use crate::common::{api, harness};
use axum::body::{Body, to_bytes};
use axum::http::{Request, StatusCode};
use serde_json::{Value, json};
use tower::ServiceExt;

fn call(account_id: &str, method: &str, mut args: Value) -> Value {
    args["accountId"] = json!(account_id);
    json!({
        "using": ["urn:ietf:params:jmap:core", "urn:ietf:params:jmap:mail"],
        "methodCalls": [[method, args, "c"]],
    })
}

fn resp_name(body: &Value) -> String {
    body["methodResponses"][0][0]
        .as_str()
        .unwrap_or("")
        .to_owned()
}
fn err_type(body: &Value) -> String {
    body["methodResponses"][0][1]["type"]
        .as_str()
        .unwrap_or("")
        .to_owned()
}

#[tokio::test]
async fn read_only_delegate_can_read_not_write() {
    let h = harness("deleg-ro").await;
    let owner = h.ts.create_user("owner-ro@example.test").await.unwrap();
    let owner_acc = h.store.for_account(h.tenant.clone(), owner.clone());
    let mid = owner_acc
        .deliver(b"From: a@x\r\nSubject: owner-secret\r\n\r\nbody\r\n")
        .await
        .unwrap();
    let owner_id = owner.to_string();

    // No grant → accountNotFound.
    let (_s, body) = api(
        &h.app,
        &h.token,
        call(&owner_id, "Mailbox/get", json!({ "ids": null })),
    )
    .await;
    assert_eq!(err_type(&body), "accountNotFound");

    // Read-only grant.
    h.ts.grant_delegate(&owner, &h.user, false, "none")
        .await
        .unwrap();

    // Can read the owner's mail...
    let (_s, body) = api(
        &h.app,
        &h.token,
        call(&owner_id, "Mailbox/get", json!({ "ids": null })),
    )
    .await;
    assert_eq!(resp_name(&body), "Mailbox/get");

    // ...but any /set is refused as read-only.
    let update = json!({ mid.to_string(): { "keywords/$flagged": true } });
    let (_s, body) = api(
        &h.app,
        &h.token,
        call(&owner_id, "Email/set", json!({ "update": update })),
    )
    .await;
    assert_eq!(
        err_type(&body),
        "accountReadOnly",
        "read-only delegate can't mutate: {body}"
    );
}

#[tokio::test]
async fn manage_delegate_writes_but_cannot_send_without_grant() {
    let h = harness("deleg-manage").await;
    let owner = h.ts.create_user("owner-mng@example.test").await.unwrap();
    let owner_acc = h.store.for_account(h.tenant.clone(), owner.clone());
    let mid = owner_acc
        .deliver(b"From: a@x\r\nSubject: s\r\n\r\nb\r\n")
        .await
        .unwrap();
    let owner_id = owner.to_string();

    // Manage access, but no send.
    h.ts.grant_delegate(&owner, &h.user, true, "none")
        .await
        .unwrap();

    // Can flag a message in the owner's mailbox.
    let update = json!({ mid.to_string(): { "keywords/$flagged": true } });
    let (_s, body) = api(
        &h.app,
        &h.token,
        call(&owner_id, "Email/set", json!({ "update": update })),
    )
    .await;
    assert!(
        body["methodResponses"][0][1]["updated"]
            .get(mid.to_string())
            .is_some(),
        "manage delegate can write: {body}",
    );

    // But cannot send (no send grant) — refused up front.
    let (_s, body) = api(
        &h.app,
        &h.token,
        call(
            &owner_id,
            "EmailSubmission/set",
            json!({ "create": { "s": { "emailId": "x" } } }),
        ),
    )
    .await;
    assert_eq!(
        body["methodResponses"][0][1]["notCreated"]["s"]["type"],
        json!("forbiddenToSend"),
    );
}

#[tokio::test]
async fn session_reflects_access_level() {
    let h = harness("deleg-sess").await;
    let send_owner = h.ts.create_user("send-owner@example.test").await.unwrap();
    let ro_owner = h.ts.create_user("ro-owner@example.test").await.unwrap();
    h.ts.grant_delegate(&send_owner, &h.user, true, "on_behalf")
        .await
        .unwrap();
    h.ts.grant_delegate(&ro_owner, &h.user, false, "none")
        .await
        .unwrap();

    let req = Request::builder()
        .method("GET")
        .uri("/.well-known/jmap")
        .header("authorization", format!("Bearer {}", h.token))
        .body(Body::empty())
        .unwrap();
    let resp = h.app.clone().oneshot(req).await.unwrap();
    let bytes = to_bytes(resp.into_body(), 1 << 20).await.unwrap();
    let session: Value = serde_json::from_slice(&bytes).unwrap();

    let send = &session["accounts"][send_owner.to_string()];
    assert_eq!(send["isPersonal"], json!(false));
    assert_eq!(send["isReadOnly"], json!(false));
    assert_eq!(send["alo:canSend"], json!(true));

    let ro = &session["accounts"][ro_owner.to_string()];
    assert_eq!(ro["isReadOnly"], json!(true));
    assert_eq!(ro["alo:canSend"], json!(false));
}

#[tokio::test]
async fn self_service_share_and_revoke() {
    let h = harness("deleg-self").await;
    // A colleague in the same tenant.
    let colleague_email = format!("colleague-{}@example.test", h.tenant);
    h.ts.create_user(&colleague_email).await.unwrap();

    // The signed-in user shares THEIR OWN mailbox with the colleague (no admin).
    let (status, _b) = post(
        &h.app,
        &h.token,
        "/jmap/delegates",
        json!({ "email": colleague_email, "canWrite": true, "sendMode": "as" }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "self-service grant");

    // It shows up in the owner's delegate list.
    let (status, body) = get(&h.app, &h.token, "/jmap/delegates").await;
    assert_eq!(status, StatusCode::OK);
    let ds = body["delegates"].as_array().unwrap();
    assert_eq!(ds.len(), 1);
    assert_eq!(ds[0]["email"], json!(colleague_email));
    assert_eq!(ds[0]["sendMode"], json!("as"));
    let colleague_id = ds[0]["id"].as_str().unwrap().to_owned();

    // Sharing with a stranger (not in the tenant) is a not-found.
    let (status, _b) = post(
        &h.app,
        &h.token,
        "/jmap/delegates",
        json!({ "email": "nobody@example.test" }),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    // Revoke.
    let (status, _b) = post(
        &h.app,
        &h.token,
        "/jmap/delegates/remove",
        json!({ "delegateId": colleague_id }),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let (_s, body) = get(&h.app, &h.token, "/jmap/delegates").await;
    assert!(body["delegates"].as_array().unwrap().is_empty());
}

#[tokio::test]
async fn folder_restricted_delegate_is_confined_to_granted_folders() {
    let h = harness("deleg-folder").await;
    let owner = h.ts.create_user("owner-fold@example.test").await.unwrap();
    let owner_acc = h.store.for_account(h.tenant.clone(), owner.clone());
    let owner_id = owner.to_string();

    // A message in the inbox (to be granted) and one moved into a private folder
    // (never granted).
    let inbox_msg = owner_acc
        .deliver(b"From: a@x\r\nSubject: inbox-msg\r\n\r\nb\r\n")
        .await
        .unwrap();
    let inbox_id = owner_acc.mailboxes_of_message(&inbox_msg).await.unwrap()[0].clone();
    let private = owner_acc
        .create_mailbox(None, "Private", None)
        .await
        .unwrap();
    let secret = owner_acc
        .deliver(b"From: a@x\r\nSubject: secret\r\n\r\nb\r\n")
        .await
        .unwrap();
    owner_acc.add_to_mailbox(&secret, &private).await.unwrap();
    owner_acc
        .remove_from_mailbox(&secret, &inbox_id)
        .await
        .unwrap();

    // Manage grant, then restricted to only the inbox folder.
    h.ts.grant_delegate(&owner, &h.user, true, "none")
        .await
        .unwrap();
    h.ts.set_delegate_folders(&owner, &h.user, &[inbox_id.to_string()])
        .await
        .unwrap();

    // Mailbox/get returns ONLY the granted folder — Private is invisible.
    let (_s, body) = api(
        &h.app,
        &h.token,
        call(&owner_id, "Mailbox/get", json!({ "ids": null })),
    )
    .await;
    let list = body["methodResponses"][0][1]["list"].as_array().unwrap();
    let ids: Vec<&str> = list.iter().filter_map(|m| m["id"].as_str()).collect();
    assert!(
        ids.contains(&inbox_id.to_string().as_str()),
        "granted folder visible: {body}"
    );
    assert!(
        !ids.contains(&private.to_string().as_str()),
        "private folder hidden: {body}"
    );

    // Fetching Private by id is NotFound (no oracle for "exists but forbidden").
    let (_s, body) = api(
        &h.app,
        &h.token,
        call(
            &owner_id,
            "Mailbox/get",
            json!({ "ids": [private.to_string()] }),
        ),
    )
    .await;
    assert!(
        body["methodResponses"][0][1]["list"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        body["methodResponses"][0][1]["notFound"][0],
        json!(private.to_string())
    );

    // Email/get on the private message → NotFound; on the inbox message → returned.
    let (_s, body) = api(
        &h.app,
        &h.token,
        call(
            &owner_id,
            "Email/get",
            json!({ "ids": [secret.to_string()] }),
        ),
    )
    .await;
    assert!(
        body["methodResponses"][0][1]["list"]
            .as_array()
            .unwrap()
            .is_empty(),
        "secret not returned: {body}"
    );
    assert_eq!(
        body["methodResponses"][0][1]["notFound"][0],
        json!(secret.to_string())
    );
    let (_s, body) = api(
        &h.app,
        &h.token,
        call(
            &owner_id,
            "Email/get",
            json!({ "ids": [inbox_msg.to_string()] }),
        ),
    )
    .await;
    assert_eq!(
        body["methodResponses"][0][1]["list"]
            .as_array()
            .unwrap()
            .len(),
        1,
        "inbox msg visible: {body}"
    );

    // Email/query (whole account) returns only the visible message.
    let (_s, body) = api(&h.app, &h.token, call(&owner_id, "Email/query", json!({}))).await;
    let qids: Vec<String> = body["methodResponses"][0][1]["ids"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|v| v.as_str().map(str::to_owned))
        .collect();
    assert!(
        qids.contains(&inbox_msg.to_string()),
        "inbox in query: {body}"
    );
    assert!(
        !qids.contains(&secret.to_string()),
        "secret excluded from query: {body}"
    );

    // Flagging the private message is refused (NotFound); the inbox one succeeds.
    let up = json!({ secret.to_string(): { "keywords/$flagged": true } });
    let (_s, body) = api(
        &h.app,
        &h.token,
        call(&owner_id, "Email/set", json!({ "update": up })),
    )
    .await;
    assert!(
        body["methodResponses"][0][1]["updated"]
            .get(secret.to_string())
            .is_none()
    );
    assert_eq!(
        body["methodResponses"][0][1]["notUpdated"][secret.to_string()]["type"],
        json!("notFound"),
        "{body}"
    );
    let up = json!({ inbox_msg.to_string(): { "keywords/$flagged": true } });
    let (_s, body) = api(
        &h.app,
        &h.token,
        call(&owner_id, "Email/set", json!({ "update": up })),
    )
    .await;
    assert!(
        body["methodResponses"][0][1]["updated"]
            .get(inbox_msg.to_string())
            .is_some(),
        "inbox flaggable: {body}"
    );

    // Moving the inbox message INTO the ungranted folder is forbidden.
    let mv = json!({ inbox_msg.to_string(): { "mailboxIds": { private.to_string(): true } } });
    let (_s, body) = api(
        &h.app,
        &h.token,
        call(&owner_id, "Email/set", json!({ "update": mv })),
    )
    .await;
    assert_eq!(
        body["methodResponses"][0][1]["notUpdated"][inbox_msg.to_string()]["type"],
        json!("forbidden"),
        "{body}"
    );

    // Destroying the private message is refused (NotFound).
    let (_s, body) = api(
        &h.app,
        &h.token,
        call(
            &owner_id,
            "Email/set",
            json!({ "destroy": [secret.to_string()] }),
        ),
    )
    .await;
    assert_eq!(
        body["methodResponses"][0][1]["notDestroyed"][secret.to_string()]["type"],
        json!("notFound"),
        "{body}"
    );

    // Restructuring the mailbox (Mailbox/set) is refused for a restricted delegate.
    let (_s, body) = api(
        &h.app,
        &h.token,
        call(
            &owner_id,
            "Mailbox/set",
            json!({ "create": { "x": { "name": "New" } } }),
        ),
    )
    .await;
    assert_eq!(
        err_type(&body),
        "accountReadOnly",
        "restricted delegate can't restructure: {body}"
    );

    // Clearing the restriction restores whole-mailbox access.
    h.ts.set_delegate_folders(&owner, &h.user, &[])
        .await
        .unwrap();
    let (_s, body) = api(
        &h.app,
        &h.token,
        call(
            &owner_id,
            "Email/get",
            json!({ "ids": [secret.to_string()] }),
        ),
    )
    .await;
    assert_eq!(
        body["methodResponses"][0][1]["list"]
            .as_array()
            .unwrap()
            .len(),
        1,
        "unrestricted again: {body}"
    );
}

#[tokio::test]
async fn folder_grant_includes_subfolders() {
    let h = harness("deleg-subfolder").await;
    let owner = h.ts.create_user("owner-sub@example.test").await.unwrap();
    let owner_acc = h.store.for_account(h.tenant.clone(), owner.clone());
    let owner_id = owner.to_string();

    // A parent folder with a child, and a message living in the child.
    let parent = owner_acc
        .create_mailbox(None, "Projects", None)
        .await
        .unwrap();
    let child = owner_acc
        .create_mailbox(Some(&parent), "Q1", None)
        .await
        .unwrap();
    let inbox_msg = owner_acc
        .deliver(b"From: a@x\r\nSubject: m\r\n\r\nb\r\n")
        .await
        .unwrap();
    let inbox_id = owner_acc.mailboxes_of_message(&inbox_msg).await.unwrap()[0].clone();
    let child_msg = owner_acc
        .deliver(b"From: a@x\r\nSubject: q1-msg\r\n\r\nb\r\n")
        .await
        .unwrap();
    owner_acc.add_to_mailbox(&child_msg, &child).await.unwrap();
    owner_acc
        .remove_from_mailbox(&child_msg, &inbox_id)
        .await
        .unwrap();

    // Grant only the PARENT folder.
    h.ts.grant_delegate(&owner, &h.user, true, "none")
        .await
        .unwrap();
    h.ts.set_delegate_folders(&owner, &h.user, &[parent.to_string()])
        .await
        .unwrap();

    // Both the parent AND the child are visible — the subfolder is inherited.
    let (_s, body) = api(
        &h.app,
        &h.token,
        call(&owner_id, "Mailbox/get", json!({ "ids": null })),
    )
    .await;
    let ids: Vec<String> = body["methodResponses"][0][1]["list"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|m| m["id"].as_str().map(str::to_owned))
        .collect();
    assert!(ids.contains(&parent.to_string()), "parent visible: {body}");
    assert!(
        ids.contains(&child.to_string()),
        "child (subfolder) inherited: {body}"
    );

    // The message in the child is visible via the inherited grant.
    let (_s, body) = api(
        &h.app,
        &h.token,
        call(
            &owner_id,
            "Email/get",
            json!({ "ids": [child_msg.to_string()] }),
        ),
    )
    .await;
    assert_eq!(
        body["methodResponses"][0][1]["list"]
            .as_array()
            .unwrap()
            .len(),
        1,
        "child message visible: {body}"
    );
}

async fn post(app: &axum::Router, token: &str, uri: &str, body: Value) -> (StatusCode, Value) {
    send_req(app, token, "POST", uri, Some(body)).await
}
async fn get(app: &axum::Router, token: &str, uri: &str) -> (StatusCode, Value) {
    send_req(app, token, "GET", uri, None).await
}
async fn send_req(
    app: &axum::Router,
    token: &str,
    method: &str,
    uri: &str,
    body: Option<Value>,
) -> (StatusCode, Value) {
    let b = body
        .map(|v| Body::from(v.to_string()))
        .unwrap_or(Body::empty());
    let req = Request::builder()
        .method(method)
        .uri(uri)
        .header("authorization", format!("Bearer {token}"))
        .header("content-type", "application/json")
        .body(b)
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    let status = resp.status();
    let bytes = to_bytes(resp.into_body(), 1 << 20).await.unwrap();
    let json = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
    (status, json)
}
