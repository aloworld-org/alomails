//! The Drive agent over its intents (ADR 0058, queue item AB.1), on the wire:
//! in a real room, against the real router and store, with a scripted model.
//!
//! What AB.1 adds is the Drive's own answer to a colleague's questions —
//! "which files do we have", "what is in that folder", "what is shared with
//! me" — and the one write that grows the tree (`create_folder`). This suite
//! holds the wave's three sentences: a read runs inside the turn and the
//! record view reaches the model as a source; a file of another tenant's or of
//! a colleague's private Drive is not among the things that can be listed; a
//! write is proposed, previewed and not run.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::time::{Duration, Instant};

use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use bytes::Bytes;
use serde_json::{Value, json};

use crate::common::model::{Seen, says, scripted_model, use_model, wants};
use crate::common::{Harness, harness, send};
use alo_store::{AccountStore, AgentProduct, DriveLocation, DriveNodeId, NewDriveFile};

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

async fn the_drive_agent(h: &Harness) -> String {
    let handle = alo_store::default_handle(AgentProduct::Drive);
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

// ---- seeding, the way the product itself does it -----------------------------

async fn a_file_for(
    acc: &AccountStore,
    loc: &DriveLocation,
    name: &str,
    bytes: &[u8],
    content_type: &str,
    parent: Option<&DriveNodeId>,
) -> DriveNodeId {
    let size = i64::try_from(bytes.len()).unwrap();
    let blob = acc
        .put_blob(Bytes::from(bytes.to_vec()), Some(content_type))
        .await
        .unwrap();
    acc.drive_create_file(
        loc,
        parent,
        &NewDriveFile {
            name: name.to_owned(),
            blob_id: blob.as_str().to_owned(),
            size,
            content_type: Some(content_type.to_owned()),
            kind: Some("file".to_owned()),
            ..NewDriveFile::default()
        },
    )
    .await
    .unwrap()
}

// ---- the reads: answered from the record, inside the tenant ------------------

/// **AB.1's headline sentence**: "@drive which files do we have?" is answered
/// from the record — the caller's own files reach the model as a source, and
/// another tenant's or a colleague's private files are not among them.
#[tokio::test]
async fn which_files_do_we_have_is_answered_from_the_record() {
    let h = harness("drive-intents-recent").await;
    a_file_for(
        &h.acc,
        &DriveLocation::Personal,
        "Handover note.md",
        b"Marta takes over the Delaunay account.",
        "text/markdown",
        None,
    )
    .await;
    a_file_for(
        &h.acc,
        &DriveLocation::Personal,
        "Price list.csv",
        b"item,price\nhosting,249",
        "text/csv",
        None,
    )
    .await;
    // A stranger's file and a colleague's private file, so the assertions
    // below are about reach and not about emptiness.
    let other = crate::common::harness_on(h.store.clone(), "drive-intents-stranger").await;
    a_file_for(
        &other.acc,
        &DriveLocation::Personal,
        "Their secret strategy.md",
        b"not yours",
        "text/markdown",
        None,
    )
    .await;
    let colleague =
        h.ts.create_user("ben@drive-intents-recent.test")
            .await
            .unwrap();
    let theirs = h.store.for_account(h.tenant.clone(), colleague);
    a_file_for(
        &theirs,
        &DriveLocation::Personal,
        "Bens private appraisal.md",
        b"not yours either",
        "text/markdown",
        None,
    )
    .await;

    let agent = the_drive_agent(&h).await;
    let room = a_room_with(&h, "files", &agent).await;
    let (model, seen) = scripted_model(vec![
        wants("recent_files", json!({}), "Let me look at the drive."),
        says("Two files: the handover note and the price list [1]."),
    ])
    .await;
    use_model(&h, &model).await;

    let answer = ask_in_room(&h, &room, "@drive which files do we have?").await;
    assert_eq!(
        answer["body"],
        "Two files: the handover note and the price list [1]."
    );
    assert!(
        answer["proposal"].is_null(),
        "a read is answered, never proposed"
    );

    // The agent was offered its verbs — reads and writes alike, rendered from
    // the intent registry.
    let prompt = offered(&seen, 0);
    for verb in [
        "recent_files",
        "list_folder",
        "shared_with_me",
        "find_file",
        "file_read",
        "create_folder",
        "file_rename",
        "file_move",
    ] {
        assert!(
            prompt.contains(&format!("- {verb}:")),
            "the prompt does not offer {verb}"
        );
    }
    // The record view came back as a source: the caller's files, nobody
    // else's.
    let sources = shown(&seen, 1);
    assert!(sources.contains("driveRecentFiles"), "{sources}");
    assert!(sources.contains("Handover note.md"), "{sources}");
    assert!(sources.contains("Price list.csv"), "{sources}");
    assert!(
        !sources.contains("Their secret strategy"),
        "another tenant's file reached the model: {sources}"
    );
    assert!(
        !sources.contains("Bens private appraisal"),
        "a colleague's private file reached the model: {sources}"
    );
}

/// "What is shared with me" is the Spaces the caller belongs to — each with
/// its files — and never a Space of another tenant's.
#[tokio::test]
async fn what_is_shared_with_me_lists_the_spaces_and_their_files() {
    let h = harness("drive-intents-shared").await;
    let space = h.acc.create_space("Marketing").await.unwrap();
    a_file_for(
        &h.acc,
        &DriveLocation::Space(space),
        "Launch plan.md",
        b"October.",
        "text/markdown",
        None,
    )
    .await;
    let other = crate::common::harness_on(h.store.clone(), "drive-intents-shared-x").await;
    let theirs = other.acc.create_space("Their secret space").await.unwrap();
    a_file_for(
        &other.acc,
        &DriveLocation::Space(theirs),
        "Their roadmap.md",
        b"not yours",
        "text/markdown",
        None,
    )
    .await;

    let agent = the_drive_agent(&h).await;
    let room = a_room_with(&h, "files", &agent).await;
    let (model, seen) = scripted_model(vec![
        wants("shared_with_me", json!({}), "Let me look at the spaces."),
        says("You are in Marketing, which holds the launch plan [1]."),
    ])
    .await;
    use_model(&h, &model).await;

    let answer = ask_in_room(&h, &room, "@drive what is shared with me?").await;
    assert_eq!(
        answer["body"],
        "You are in Marketing, which holds the launch plan [1]."
    );
    let sources = shown(&seen, 1);
    assert!(sources.contains("driveShared"), "{sources}");
    assert!(sources.contains("Marketing"), "{sources}");
    assert!(sources.contains("Launch plan.md"), "{sources}");
    assert!(sources.contains("\"spaceCount\":1"), "{sources}");
    assert!(
        !sources.contains("Their secret space") && !sources.contains("Their roadmap"),
        "another tenant's space reached the model: {sources}"
    );
}

// ---- the write: proposed, previewed, not run ---------------------------------

#[tokio::test]
async fn creating_a_folder_is_proposed_and_not_run() {
    let h = harness("drive-intents-folder").await;
    let agent = the_drive_agent(&h).await;
    let room = a_room_with(&h, "files", &agent).await;
    let (model, _seen) = scripted_model(vec![wants(
        "create_folder",
        json!({ "name": "Contracts" }),
        "I'll make a Contracts folder.",
    )])
    .await;
    use_model(&h, &model).await;

    let answer = ask_in_room(&h, &room, "@drive make a Contracts folder").await;
    assert!(
        !answer["proposal"].is_null(),
        "a write is a proposal: {answer}"
    );
    assert_eq!(answer["proposal"]["tool"], "create_folder");
    // Nothing ran without a tap: the Drive's top level has no such folder.
    let (status, body) = get(&h.app, &h.token, "/drive/list").await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert!(
        !body["nodes"]
            .as_array()
            .unwrap()
            .iter()
            .any(|n| n["name"] == "Contracts"),
        "the folder was created before the tap: {body}"
    );
}

// ---- arguments and refusals, over the approval route -------------------------

/// The new verbs against the real Drive: a folder is created once and refused
/// the second time by name; a folder's contents are listed by its name; a
/// folder that is not there is refused with the folders that are; and
/// `recent_files` lists files, never folders.
#[tokio::test]
async fn the_folder_verbs_run_against_the_real_drive() {
    let h = harness("drive-intents-verbs").await;
    a_file_for(
        &h.acc,
        &DriveLocation::Personal,
        "Handover note.md",
        b"Marta takes over.",
        "text/markdown",
        None,
    )
    .await;

    let (status, body) = run(&h, "create_folder", json!({ "name": "Contracts" })).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["result"]["kind"], "driveFolderCreated");
    assert_eq!(body["result"]["folder"]["name"], "Contracts");
    assert_eq!(body["result"]["folder"]["kind"], "folder");

    // The same name again is refused by name, not made unique.
    let (status, body) = run(&h, "create_folder", json!({ "name": "Contracts" })).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
    assert!(
        why(&body).starts_with("there is already a Contracts"),
        "{body}"
    );

    // The top level lists the folder and the file both.
    let (status, body) = run(&h, "list_folder", json!({})).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["result"]["kind"], "driveFolder");
    let names: Vec<String> = body["result"]["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|n| n["name"].as_str().unwrap().to_owned())
        .collect();
    assert!(names.contains(&"Contracts".to_owned()), "{names:?}");
    assert!(names.contains(&"Handover note.md".to_owned()), "{names:?}");

    // A folder by name; empty is an answer.
    let (status, body) = run(&h, "list_folder", json!({ "folder": "Contracts" })).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["result"]["folder"]["name"], "Contracts");
    assert_eq!(body["result"]["nodeCount"], 0, "{body}");

    // A folder that is not there is refused, and the refusal lists what is.
    let (status, body) = run(&h, "list_folder", json!({ "folder": "Nope" })).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
    assert!(why(&body).contains("you have: Contracts"), "{body}");

    // …and a subfolder lands inside its parent, found by the parent's name.
    let (status, body) = run(
        &h,
        "create_folder",
        json!({ "name": "Signed", "folder": "Contracts" }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["result"]["parent"]["name"], "Contracts");
    let (status, body) = run(&h, "list_folder", json!({ "folder": "Contracts" })).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["result"]["nodes"][0]["name"], "Signed", "{body}");

    // `recent_files` is files: the folders are not in it.
    let (status, body) = run(&h, "recent_files", json!({})).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let files: Vec<String> = body["result"]["files"]
        .as_array()
        .unwrap()
        .iter()
        .map(|n| n["name"].as_str().unwrap().to_owned())
        .collect();
    assert_eq!(files, ["Handover note.md"], "{body}");
}
