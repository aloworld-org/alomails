//! The Docs agent over its intents (ADR 0058, queue item AB.2), on the wire:
//! in a real room, against the real router and store, with a scripted model.
//!
//! What AB.2 adds is the Docs' own answer to a colleague's question — "which
//! documents exist" — and the one write that starts a document without
//! putting a word in it (`create_document`). This suite holds the wave's
//! three sentences: a read runs inside the turn and the record view reaches
//! the model as a source; a document of another tenant's or of a colleague's
//! private Drive is not among the things that can be listed; a write is
//! proposed, previewed and not run.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::time::{Duration, Instant};

use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use bytes::Bytes;
use serde_json::{Value, json};

use alo_store::{AccountStore, AgentProduct, DriveLocation, DriveNodeId, NewDriveFile};

use crate::common;
use crate::common::model::{Seen, says, scripted_model, use_model, wants};
use crate::common::{Harness, harness, send};

// ---- request helpers ---------------------------------------------------------

async fn post(app: &Router, token: &str, uri: &str, body: Value) -> (StatusCode, Value) {
    let req = Request::builder()
        .method("POST")
        .uri(uri)
        .header("content-type", "application/json")
        .header("authorization", format!("Bearer {token}"))
        .body(Body::from(body.to_string()))
        .unwrap();
    send(app, req).await
}

async fn get(app: &Router, token: &str, uri: &str) -> (StatusCode, Value) {
    let req = Request::builder()
        .uri(uri)
        .header("authorization", format!("Bearer {token}"))
        .body(Body::empty())
        .unwrap();
    send(app, req).await
}

async fn the_docs_agent(h: &Harness) -> String {
    let handle = alo_store::default_handle(AgentProduct::Docs);
    let (status, body) = get(&h.app, &h.token, "/chat/agents").await;
    assert_eq!(status, StatusCode::OK, "{body}");
    body["agents"]
        .as_array()
        .unwrap()
        .iter()
        .find(|agent| agent["handle"] == handle)
        .unwrap_or_else(|| panic!("no @{handle} among this tenant's agents: {body}"))["id"]
        .as_str()
        .unwrap()
        .to_owned()
}

async fn a_room_with(h: &Harness, name: &str, agent: &str) -> String {
    let (status, body) = post(
        &h.app,
        &h.token,
        "/chat/channels",
        json!({ "kind": "channel", "name": name, "visibility": "public" }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let channel = body["id"].as_str().unwrap().to_owned();
    let (status, body) = post(
        &h.app,
        &h.token,
        &format!("/chat/channels/{channel}/agents"),
        json!({ "agent": agent }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    channel
}

async fn messages(h: &Harness, channel: &str) -> Vec<Value> {
    let (status, body) = get(
        &h.app,
        &h.token,
        &format!("/chat/channels/{channel}/messages"),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    body["messages"].as_array().unwrap().clone()
}

/// Says something in the room and waits for the agent's reply.
async fn ask_in_room(h: &Harness, channel: &str, question: &str) -> Value {
    let (status, body) = post(
        &h.app,
        &h.token,
        &format!("/chat/channels/{channel}/messages"),
        json!({ "body": question }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let deadline = Instant::now() + Duration::from_secs(20);
    loop {
        let spoken = messages(h, channel)
            .await
            .into_iter()
            .find(|m| m["authorKind"] == "agent");
        if let Some(message) = spoken {
            return message;
        }
        assert!(Instant::now() < deadline, "the agent never spoke");
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

/// One approved tool run over the ordinary approval route — the same path the
/// command palette's button takes. The tests that use it are about **arguments
/// and refusals**, which a chat turn cannot vary as finely as they need.
async fn run(h: &Harness, tool: &str, args: Value) -> (StatusCode, Value) {
    post(
        &h.app,
        &h.token,
        "/ai/agent/execute",
        json!({ "tool": tool, "args": args }),
    )
    .await
}

/// The `detail` of a refusal, which is the sentence the client shows.
fn why(body: &Value) -> String {
    body["detail"].as_str().unwrap_or_default().to_owned()
}

/// What the model was shown on call `n` — the user turn, where the grounding
/// and the tool results live.
fn shown(seen: &Seen, n: usize) -> String {
    let asked = seen.lock().unwrap().clone();
    let messages = asked
        .get(n)
        .unwrap_or_else(|| panic!("the model was not called {} times", n + 1))["messages"]
        .as_array()
        .unwrap()
        .clone();
    messages.last().unwrap()["content"]
        .as_str()
        .unwrap()
        .to_owned()
}

/// The system prompt of the model's `n`th call.
fn offered(seen: &Seen, n: usize) -> String {
    let asked = seen.lock().unwrap().clone();
    asked[n]["messages"][0]["content"]
        .as_str()
        .unwrap()
        .to_owned()
}

// ---- seeding, the way the editor's own save does it --------------------------

/// A document in a Drive: a blob holding the editor's block array, a `doc`
/// node pointing at it.
async fn a_doc_for(acc: &AccountStore, name: &str, text: &str) -> DriveNodeId {
    let blocks = json!([
        {"id": "b1", "type": "paragraph", "props": {},
         "content": [{"type": "text", "text": text, "styles": {}}],
         "children": []}
    ]);
    let bytes = serde_json::to_vec(&blocks).unwrap();
    let size = i64::try_from(bytes.len()).unwrap();
    let blob = acc
        .put_blob(Bytes::from(bytes), Some("application/json"))
        .await
        .unwrap();
    acc.drive_create_file(
        &DriveLocation::Personal,
        None,
        &NewDriveFile {
            name: name.to_owned(),
            blob_id: blob.as_str().to_owned(),
            size,
            content_type: Some("application/json".to_owned()),
            kind: Some("doc".to_owned()),
            ..NewDriveFile::default()
        },
    )
    .await
    .unwrap()
}

// ---- the read: answered from the record, inside the tenant -------------------

/// **AB.2's headline sentence**: "@docs which documents exist?" is answered
/// from the record — the caller's own documents reach the model as a source,
/// and another tenant's or a colleague's private documents are not among them.
#[tokio::test]
async fn which_documents_exist_is_answered_from_the_record() {
    let h = harness("docs-intents-list").await;
    a_doc_for(&h.acc, "Handover", "Marta takes over the Delaunay account.").await;
    a_doc_for(
        &h.acc,
        "Terms of engagement",
        "Invoices are due in 30 days.",
    )
    .await;
    // A stranger's document and a colleague's private one, so the assertions
    // below are about reach and not about emptiness.
    let other = common::harness_on(h.store.clone(), "docs-intents-stranger").await;
    a_doc_for(&other.acc, "Their secret plan", "not yours").await;
    let colleague =
        h.ts.create_user("ben@docs-intents-list.test")
            .await
            .unwrap();
    let theirs = h.store.for_account(h.tenant.clone(), colleague);
    a_doc_for(&theirs, "Bens private notes", "not yours either").await;

    let agent = the_docs_agent(&h).await;
    let room = a_room_with(&h, "documents", &agent).await;
    let (model, seen) = scripted_model(vec![
        wants("list_documents", json!({}), "Let me look at the documents."),
        says("Two documents: the handover and the terms of engagement [1]."),
    ])
    .await;
    use_model(&h, &model).await;

    let answer = ask_in_room(&h, &room, "@docs which documents exist?").await;
    assert_eq!(
        answer["body"],
        "Two documents: the handover and the terms of engagement [1]."
    );
    assert!(
        answer["proposal"].is_null(),
        "a read is answered, never proposed"
    );

    // The agent was offered its verbs — reads and writes alike, rendered from
    // the intent registry — and no other product's.
    let prompt = offered(&seen, 0);
    for verb in [
        "list_documents",
        "doc_read",
        "doc_answer",
        "create_document",
        "doc_draft_section",
        "doc_rewrite",
    ] {
        assert!(
            prompt.contains(&format!("- {verb}:")),
            "the prompt does not offer {verb}"
        );
    }
    assert!(
        !prompt.contains("- sheet_answer:") && !prompt.contains("- file_read:"),
        "another product's tools reached the Docs agent: {prompt}"
    );
    // The record view came back as a source: the caller's documents, nobody
    // else's.
    let sources = shown(&seen, 1);
    assert!(sources.contains("docsList"), "{sources}");
    assert!(sources.contains("Handover"), "{sources}");
    assert!(sources.contains("Terms of engagement"), "{sources}");
    assert!(
        !sources.contains("Their secret plan"),
        "another tenant's document reached the model: {sources}"
    );
    assert!(
        !sources.contains("Bens private notes"),
        "a colleague's private document reached the model: {sources}"
    );
}

// ---- the write: proposed, previewed, not run ---------------------------------

#[tokio::test]
async fn creating_a_document_is_proposed_and_not_run() {
    let h = harness("docs-intents-create").await;
    let agent = the_docs_agent(&h).await;
    let room = a_room_with(&h, "documents", &agent).await;
    let (model, _seen) = scripted_model(vec![wants(
        "create_document",
        json!({ "title": "Handover" }),
        "I'll start a Handover document.",
    )])
    .await;
    use_model(&h, &model).await;

    let answer = ask_in_room(&h, &room, "@docs start a document called Handover").await;
    assert!(
        !answer["proposal"].is_null(),
        "a write is a proposal: {answer}"
    );
    assert_eq!(answer["proposal"]["tool"], "create_document");
    // Nothing ran without a tap: the Drive holds no such document.
    let docs = h.acc.drive_docs(50).await.unwrap();
    assert!(
        !docs.iter().any(|d| d.name == "Handover"),
        "the document was created before the tap: {docs:?}"
    );
}

// ---- arguments and refusals, over the approval route -------------------------

/// The new verbs against the real Drive: a document is created once and
/// refused the second time by name; the list shows it, by folder too; a
/// folder that is not there is refused with the folders that are; and the
/// created document opens as an empty document, not an error.
#[tokio::test]
async fn the_document_verbs_run_against_the_real_drive() {
    let h = harness("docs-intents-verbs").await;
    a_doc_for(
        &h.acc,
        "Terms of engagement",
        "Invoices are due in 30 days.",
    )
    .await;

    let (status, body) = run(&h, "create_document", json!({ "title": "Handover" })).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["result"]["kind"], "docCreated");
    assert_eq!(body["result"]["document"]["name"], "Handover");
    assert_eq!(body["result"]["document"]["kind"], "doc");

    // The same name again is refused by name, not made unique.
    let (status, body) = run(&h, "create_document", json!({ "title": "Handover" })).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
    assert!(
        why(&body).starts_with("there is already a Handover"),
        "{body}"
    );

    // The list holds both, newest first.
    let (status, body) = run(&h, "list_documents", json!({})).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["result"]["kind"], "docsList");
    let names: Vec<String> = body["result"]["documents"]
        .as_array()
        .unwrap()
        .iter()
        .map(|d| d["name"].as_str().unwrap().to_owned())
        .collect();
    assert!(names.contains(&"Handover".to_owned()), "{names:?}");
    assert!(
        names.contains(&"Terms of engagement".to_owned()),
        "{names:?}"
    );

    // A document lands inside a folder, found by the folder's name.
    h.acc
        .drive_create_folder(&DriveLocation::Personal, None, "Contracts")
        .await
        .unwrap();
    let (status, body) = run(
        &h,
        "create_document",
        json!({ "title": "Signed terms", "folder": "Contracts" }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["result"]["parent"]["name"], "Contracts");

    // The folder's own list is just that document.
    let (status, body) = run(&h, "list_documents", json!({ "folder": "Contracts" })).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["result"]["folder"]["name"], "Contracts");
    assert_eq!(body["result"]["total"], 1, "{body}");
    assert_eq!(body["result"]["documents"][0]["name"], "Signed terms");

    // A folder nobody has is refused with the folders that exist.
    let (status, body) = run(&h, "list_documents", json!({ "folder": "Nope" })).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
    assert!(why(&body).contains("you have: Contracts"), "{body}");

    // The created document opens as an empty document — the same read the
    // editor's own path serves, not an error.
    let (status, body) = run(&h, "doc_read", json!({ "document": "Handover" })).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["result"]["kind"], "docRead");
    assert_eq!(body["result"]["document"]["blocks"], 0, "{body}");
}
