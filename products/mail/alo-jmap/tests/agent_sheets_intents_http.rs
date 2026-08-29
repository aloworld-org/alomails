//! The Sheets agent over its intents (ADR 0058, queue item AB.3), on the
//! wire: in a real room, against the real router and store, with a scripted
//! model.
//!
//! What AB.3 adds is the Sheets' own answer to a colleague's question —
//! "which spreadsheets do we have" (`list_spreadsheets`) — beside the five
//! tools A2.2 built, now rendered from the intent registry. This suite holds
//! the wave's three sentences: a read runs inside the turn and the record
//! view reaches the model as a source; a spreadsheet of another tenant's or
//! of a colleague's private Drive is not among the things that can be listed;
//! a write is proposed, previewed and not run. The deep behaviour of the five
//! kept tools — addresses cited, formulas refused as facts, tidying bounded —
//! stays proven by `agent_sheets_http.rs`, which runs the same executors.

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

async fn the_sheet_agent(h: &Harness) -> String {
    let handle = alo_store::default_handle(AgentProduct::Sheets);
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

/// A spreadsheet in a Drive: a blob holding the editor's own snapshot, a
/// `sheet` node pointing at it.
async fn a_sheet_for(acc: &AccountStore, parent: Option<&DriveNodeId>, name: &str) -> DriveNodeId {
    let snapshot = json!({
        "id": "wb-1",
        "name": name,
        "sheetOrder": ["sheet-1"],
        "sheets": {
            "sheet-1": {
                "id": "sheet-1",
                "name": "Sheet1",
                "cellData": {
                    "0": {"0": {"v": "Amount", "t": 1}},
                    "1": {"0": {"v": 1200, "t": 2}}
                }
            }
        }
    });
    let bytes = serde_json::to_vec(&snapshot).unwrap();
    let size = i64::try_from(bytes.len()).unwrap();
    let blob = acc
        .put_blob(Bytes::from(bytes), Some("application/json"))
        .await
        .unwrap();
    acc.drive_create_file(
        &DriveLocation::Personal,
        parent,
        &NewDriveFile {
            name: name.to_owned(),
            blob_id: blob.as_str().to_owned(),
            size,
            content_type: Some("application/json".to_owned()),
            kind: Some("sheet".to_owned()),
            ..NewDriveFile::default()
        },
    )
    .await
    .unwrap()
}

// ---- the read: answered from the record, inside the tenant -------------------

/// **AB.3's headline sentence**: "@sheets which spreadsheets do we have?" is
/// answered from the record — the caller's own workbooks reach the model as a
/// source, and another tenant's or a colleague's private ones are not among
/// them.
#[tokio::test]
async fn which_spreadsheets_exist_is_answered_from_the_record() {
    let h = harness("sheets-intents-list").await;
    a_sheet_for(&h.acc, None, "Q1 figures").await;
    a_sheet_for(&h.acc, None, "Price list").await;
    // A stranger's workbook and a colleague's private one, so the assertions
    // below are about reach and not about emptiness.
    let other = common::harness_on(h.store.clone(), "sheets-intents-stranger").await;
    a_sheet_for(&other.acc, None, "Their secret figures").await;
    let colleague =
        h.ts.create_user("ben@sheets-intents-list.test")
            .await
            .unwrap();
    let theirs = h.store.for_account(h.tenant.clone(), colleague);
    a_sheet_for(&theirs, None, "Bens private numbers").await;

    let agent = the_sheet_agent(&h).await;
    let room = a_room_with(&h, "figures", &agent).await;
    let (model, seen) = scripted_model(vec![
        wants("list_spreadsheets", json!({}), "Let me look at the drive."),
        says("Two spreadsheets: the Q1 figures and the price list [1]."),
    ])
    .await;
    use_model(&h, &model).await;

    let answer = ask_in_room(&h, &room, "@sheets which spreadsheets do we have?").await;
    assert_eq!(
        answer["body"],
        "Two spreadsheets: the Q1 figures and the price list [1]."
    );
    assert!(
        answer["proposal"].is_null(),
        "a read is answered, never proposed"
    );

    // The agent was offered its verbs — reads and writes alike, rendered from
    // the intent registry — and no other product's.
    let prompt = offered(&seen, 0);
    for verb in [
        "list_spreadsheets",
        "sheet_read",
        "sheet_answer",
        "sheet_formula_explain",
        "sheet_write_formula",
        "sheet_clean_column",
    ] {
        assert!(
            prompt.contains(&format!("- {verb}:")),
            "the prompt does not offer {verb}"
        );
    }
    assert!(
        !prompt.contains("- doc_read:") && !prompt.contains("- file_read:"),
        "another product's tools reached the Sheets agent: {prompt}"
    );
    // The record view came back as a source: the caller's workbooks, nobody
    // else's.
    let sources = shown(&seen, 1);
    assert!(sources.contains("sheetsList"), "{sources}");
    assert!(sources.contains("Q1 figures"), "{sources}");
    assert!(sources.contains("Price list"), "{sources}");
    assert!(
        !sources.contains("Their secret figures"),
        "another tenant's spreadsheet reached the model: {sources}"
    );
    assert!(
        !sources.contains("Bens private numbers"),
        "a colleague's private spreadsheet reached the model: {sources}"
    );
}

// ---- the write: proposed, previewed, not run ---------------------------------

#[tokio::test]
async fn writing_a_formula_is_proposed_and_not_run() {
    let h = harness("sheets-intents-write").await;
    let node = a_sheet_for(&h.acc, None, "Q1 figures").await;
    let agent = the_sheet_agent(&h).await;
    let room = a_room_with(&h, "figures", &agent).await;
    let (model, _seen) = scripted_model(vec![wants(
        "sheet_write_formula",
        json!({ "cells": [{ "cell": "A3", "formula": "=SUM(A2:A2)" }] }),
        "I'll put a total under the amounts.",
    )])
    .await;
    use_model(&h, &model).await;

    let answer = ask_in_room(&h, &room, "@sheets add up the amounts column").await;
    assert!(
        !answer["proposal"].is_null(),
        "a write is a proposal: {answer}"
    );
    assert_eq!(answer["proposal"]["tool"], "sheet_write_formula");
    // Nothing ran without a tap: the stored workbook still has no formula.
    let stored = h.acc.drive_node(&node).await.unwrap().unwrap();
    let blob = alo_store::BlobId::new(stored.blob_id.unwrap());
    let bytes = h.acc.blob_bytes_for_send(&blob).await.unwrap();
    assert!(
        !String::from_utf8(bytes.to_vec()).unwrap().contains("=SUM"),
        "the formula was written before the tap"
    );
}

// ---- arguments and refusals, over the approval route -------------------------

/// The new verb against the real Drive: the listing holds the caller's
/// workbooks and only workbooks, newest first; a folder narrows it by name; a
/// folder nobody has is refused with the folders that exist.
#[tokio::test]
async fn the_listing_verb_runs_against_the_real_drive() {
    let h = harness("sheets-intents-verbs").await;
    a_sheet_for(&h.acc, None, "Q1 figures").await;
    a_sheet_for(&h.acc, None, "Price list").await;
    // A document beside them, so the listing is about spreadsheets and not
    // about everything the Drive holds.
    let doc = serde_json::to_vec(&json!([])).unwrap();
    let blob = h
        .acc
        .put_blob(Bytes::from(doc), Some("application/json"))
        .await
        .unwrap();
    h.acc
        .drive_create_file(
            &DriveLocation::Personal,
            None,
            &NewDriveFile {
                name: "Handover note".to_owned(),
                blob_id: blob.as_str().to_owned(),
                size: 2,
                content_type: Some("application/json".to_owned()),
                kind: Some("doc".to_owned()),
                ..NewDriveFile::default()
            },
        )
        .await
        .unwrap();

    let (status, body) = run(&h, "list_spreadsheets", json!({})).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["result"]["kind"], "sheetsList");
    let names: Vec<String> = body["result"]["spreadsheets"]
        .as_array()
        .unwrap()
        .iter()
        .map(|node| node["name"].as_str().unwrap().to_owned())
        .collect();
    assert!(names.contains(&"Q1 figures".to_owned()), "{names:?}");
    assert!(names.contains(&"Price list".to_owned()), "{names:?}");
    assert!(
        !names.contains(&"Handover note".to_owned()),
        "a document is not a spreadsheet: {names:?}"
    );

    // A spreadsheet inside a folder is found by the folder's name, and the
    // folder's own list is just that spreadsheet.
    let folder = h
        .acc
        .drive_create_folder(&DriveLocation::Personal, None, "Finance")
        .await
        .unwrap();
    a_sheet_for(&h.acc, Some(&folder), "Budget").await;
    let (status, body) = run(&h, "list_spreadsheets", json!({ "folder": "Finance" })).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["result"]["folder"]["name"], "Finance");
    assert_eq!(body["result"]["total"], 1, "{body}");
    assert_eq!(body["result"]["spreadsheets"][0]["name"], "Budget");

    // A folder nobody has is refused with the folders that exist.
    let (status, body) = run(&h, "list_spreadsheets", json!({ "folder": "Nope" })).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
    assert!(why(&body).contains("you have: Finance"), "{body}");

    // The listed workbook opens by name through the kept read — one registry,
    // one resolver, the same record.
    let (status, body) = run(&h, "sheet_read", json!({ "workbook": "Budget" })).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["result"]["kind"], "sheetRead");
    assert_eq!(body["result"]["workbook"]["name"], "Budget");
}
