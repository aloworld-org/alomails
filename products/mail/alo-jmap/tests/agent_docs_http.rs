//! **The Docs agent, end to end** (A2.3) — the sentences ADR 0034 and ADR 0047
//! leave a document agent to prove, each asked the way a person asks it and
//! answered the way a person reads it:
//!
//! - `@docs what do we say about payment terms?` — answered from the passages
//!   of the document, **with the block and the section each came from**, and
//!   **no button in between**;
//! - a section the agent drafts **waits for a tap**, and when it lands it lands
//!   as a new Drive version of the same document, beside the block it was put
//!   after, with everything else in the document untouched;
//! - a rewrite replaces **the words of blocks that already exist and nothing
//!   else about them** — the kind, the level and the styling survive it — and a
//!   block whose content is a table is refused by name;
//! - **a translation is that same rewrite**: the whole document proposed once,
//!   in the new language, with the figures carried across unchanged.
//!
//! And the two isolation sentences the wave holds every agent to: a document of
//! another tenant's cannot be named, and neither can a colleague's.
//!
//! Everything goes through the product's own path: the tenant's agents are the
//! ones `GET /chat/agents` seeds (A1.5), the room is made over HTTP, the agent
//! joins it over HTTP, and the question is an ordinary chat message. The
//! document is a real Drive node whose blob is the same BlockNote block array
//! `web/src/drive/DocEditor.tsx` writes.
//!
//! **No live model is ever called**, here or anywhere in this workspace's tests
//! (the loop's standing rail): the tenant's AI backend is the scripted local
//! socket in `common::model`. The assertions below are about the bytes the model
//! was *shown*, which is where an answer read out of the document and a
//! plausible guess differ.
//!
//! Run the transcript with
//! `cargo nextest run -p alo-jmap --test agent_docs_http --no-capture`.

#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use std::time::{Duration, Instant};

use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use bytes::Bytes;
use serde_json::{Value, json};

use alo_store::{AgentProduct, DriveLocation, DriveNodeId, NewDriveFile};
use common::model::{Seen, says, scripted_model, use_model, wants};
use common::{Harness, harness, send};

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

/// The id of the tenant's own Docs agent, out of the set a first look at
/// `GET /chat/agents` seeds (A1.5). Nothing here registers a handle: an agent
/// this test could not find is an agent a person could not find either.
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

/// A room, with that agent in it — both over HTTP, as a person makes them.
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

/// Every message in the room, newest first as the route answers.
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

/// The asker's own tap on a proposal — the only thing that makes a change happen.
async fn approve(h: &Harness, proposal: &str) -> Value {
    let (status, decided) = post(
        &h.app,
        &h.token,
        &format!("/chat/proposals/{proposal}"),
        json!({ "approve": true }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{decided}");
    decided
}

/// One approved tool run over the ordinary approval route — `POST
/// /ai/agent/execute`, the same path the command palette's button takes, which
/// is an approval the caller gave with their own session.
///
/// The tests that use it are about **arguments and refusals**, which a chat turn
/// cannot vary as finely as they need; the chat path itself is what the room
/// exercises.
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

/// What the model was shown on call `n` — the user turn of the recorded request,
/// which is where the grounding and the tool results live.
fn shown(seen: &Seen, n: usize) -> String {
    turn_content(seen, n, false)
}

/// The system prompt of call `n` — who the agent was told it is, and which tools
/// it was offered (A1.2).
fn offered(seen: &Seen, n: usize) -> String {
    turn_content(seen, n, true)
}

fn turn_content(seen: &Seen, n: usize, system: bool) -> String {
    let asked = seen.lock().unwrap().clone();
    let messages = asked
        .get(n)
        .unwrap_or_else(|| panic!("the model was not called {} times", n + 1))["messages"]
        .as_array()
        .unwrap()
        .clone();
    let message = if system {
        messages.first().unwrap()
    } else {
        messages.last().unwrap()
    };
    message["content"].as_str().unwrap().to_owned()
}

/// Prints one exchange so the queue item's "record the actual request and
/// response" is a copy of a run rather than a claim about one.
fn transcript(title: &str, lines: &[String]) {
    println!("\n===== A2.3 TRANSCRIPT: {title} =====");
    for line in lines {
        println!("{line}");
    }
    println!("===== end: {title} =====\n");
}

// ---- the document under test --------------------------------------------------

/// A document the way a real one is: headings, prose under them, a bold run
/// inside a sentence, a list with a nested paragraph, and a table. Every one of
/// those is something the agent must handle rather than trip on.
fn blocks() -> Value {
    json!([
        {"id": "b1", "type": "heading", "props": {"level": 1, "textColor": "default"},
         "content": [{"type": "text", "text": "Terms of engagement", "styles": {}}],
         "children": []},
        {"id": "b2", "type": "heading", "props": {"level": 2},
         "content": [{"type": "text", "text": "Payment terms", "styles": {}}],
         "children": []},
        {"id": "b3", "type": "paragraph", "props": {"textAlignment": "left"},
         "content": [
            {"type": "text", "text": "Invoices are due within ", "styles": {}},
            {"type": "text", "text": "30 days", "styles": {"bold": true}},
            {"type": "text", "text": " of issue.", "styles": {}}
         ],
         "children": []},
        {"id": "b4", "type": "bulletListItem", "props": {},
         "content": [{"type": "text", "text": "Late payment is charged monthly.", "styles": {}}],
         "children": [
            {"id": "b5", "type": "paragraph", "props": {},
             "content": [{"type": "text", "text": "See the annex for the rate.", "styles": {}}],
             "children": []}
         ]},
        {"id": "b6", "type": "table", "props": {},
         "content": {"type": "tableContent", "rows": [
            {"cells": [[{"type": "text", "text": "Region", "styles": {}}]]}
         ]},
         "children": []}
    ])
}

/// Writes that document into the caller's own Drive as a `doc` node, exactly as
/// the editor's own save does — a blob, then a node pointing at it.
async fn a_document(h: &Harness, name: &str) -> DriveNodeId {
    a_document_for(&h.acc, name).await
}

async fn a_document_for(acc: &alo_store::AccountStore, name: &str) -> DriveNodeId {
    let bytes = serde_json::to_vec(&blocks()).unwrap();
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

/// The document as it stands in Drive right now, read back through the store the
/// way the editor would read it.
async fn stored(h: &Harness, node: &DriveNodeId) -> Value {
    let node = h.acc.drive_node(node).await.unwrap().unwrap();
    let blob = alo_store::BlobId::new(node.blob_id.unwrap());
    let bytes = h.acc.blob_bytes_for_send(&blob).await.unwrap();
    serde_json::from_slice(&bytes).unwrap()
}

// ---- the question, on the wire -----------------------------------------------

/// **The wave's question, end to end.** What lands in the room is the answer,
/// grounded in the passages of the document and carrying the block and the
/// section each came from — and there is no button in between, because asking
/// what a document says does not change it.
#[tokio::test]
async fn the_docs_agent_answers_from_the_document_and_cites_it_with_no_button_in_between() {
    let h = harness("agent-a23-answer").await;
    let doc = a_document(&h, "Terms of engagement").await;
    const ANSWER: &str =
        "Under Payment terms, invoices are due within 30 days of issue (block b3) [1].";
    let (base, seen) = scripted_model(vec![
        wants(
            "doc_answer",
            json!({ "question": "payment terms" }),
            "Let me look in the document.",
        ),
        says(ANSWER),
    ])
    .await;
    use_model(&h, &base).await;
    let agent = the_docs_agent(&h).await;
    let channel = a_room_with(&h, "the contract", &agent).await;

    const QUESTION: &str = "@docs what do we say about payment terms?";
    let spoken = ask_in_room(&h, &channel, QUESTION).await;

    // The answer, in the room, said once.
    assert_eq!(spoken["body"], json!(ANSWER));
    assert_eq!(spoken["authorKind"], json!("agent"));
    let room = messages(&h, &channel).await;
    assert_eq!(
        room.iter().filter(|m| m["authorKind"] == "agent").count(),
        1
    );
    // **No button in between** — not on the answer, and not on anything else in
    // the room. Asking what a document says is a lookup, not a change.
    for message in &room {
        assert_eq!(
            message["proposal"],
            Value::Null,
            "asking what a document says must never produce a proposal: {message}"
        );
    }

    assert_eq!(
        seen.lock().unwrap().len(),
        2,
        "a read costs one further call"
    );
    let first = shown(&seen, 0);
    let second = shown(&seen, 1);
    let system = offered(&seen, 0);
    assert!(
        system.contains("- doc_answer:"),
        "the Docs agent is offered its own reading tool: {system}"
    );
    assert!(
        !system.contains("- sheet_answer:") && !system.contains("- site_answer:"),
        "and only its own product's tools (A1.2): {system}"
    );
    // Its grounding is empty on purpose (A1.3): Docs reaches its records through
    // the tool, so the first call carries the question and no snippets.
    assert!(
        first.contains("Sources:\n\n") || first.trim_end().ends_with("Sources:"),
        "the question is not answered from a search snippet: {first}"
    );
    // **The passages, with the block and the section.** This is the assertion
    // the item is named after: an answer a person can find in their own file.
    assert!(
        second.contains("tool result \"doc_answer\""),
        "the tool's own result must be among the sources: {second}"
    );
    assert!(second.contains("\"kind\":\"docAnswer\""), "{second}");
    assert!(
        second.contains("\"section\":\"Payment terms\""),
        "a passage comes back captioned by the heading above it: {second}"
    );
    // **The sentence that answers the question**, which holds none of the words
    // that were asked about: it came with the heading it sits under, the way a
    // spreadsheet answer comes with its whole row.
    assert!(second.contains("\"block\":\"b3\""), "{second}");
    assert!(
        second.contains("Invoices are due within 30 days of issue."),
        "the sentence itself, its runs joined exactly as they were typed: {second}"
    );
    // …and what matched is distinguished from what came along with it, so the
    // search never looks like it found more than it did.
    assert!(
        second.contains("\"text\":\"Payment terms\",\"truncated\":false,\"matched\":true")
            || second.contains("\"matched\":true"),
        "{second}"
    );
    assert!(second.contains("\"matched\":false"), "{second}");

    // Audited as a read — the agent's, the room's, and successful.
    let runs = h.acc.agent_tool_runs(50).await.unwrap();
    assert_eq!(runs.len(), 1, "{runs:?}");
    assert_eq!(runs[0].tool, "doc_answer");
    assert_eq!(runs[0].effect, "read");
    assert!(runs[0].ok);
    let record = h.acc.agent_records().await.unwrap();
    let record = record.get(agent.as_str()).unwrap();
    assert_eq!(record.reads, 1);
    assert_eq!(record.answers, 1);
    assert_eq!(record.actions, 0);

    // Nothing was written: asking a question about a document leaves it on the
    // version it was on.
    assert_eq!(h.acc.drive_versions(&doc).await.unwrap().len(), 1);
    assert_eq!(stored(&h, &doc).await, blocks());

    transcript(
        QUESTION,
        &[
            format!("POST /chat/channels/{channel}/messages"),
            format!("     {}", json!({ "body": QUESTION })),
            "--- what the model was shown (call 1 of 2, user turn) ---".to_owned(),
            first,
            "--- what the model replied (call 1) ---".to_owned(),
            wants(
                "doc_answer",
                json!({ "question": "payment terms" }),
                "Let me look in the document.",
            ),
            "--- what the model was shown (call 2 of 2, user turn) ---".to_owned(),
            second,
            "--- what the model replied (call 2) ---".to_owned(),
            says(ANSWER),
            "--- GET /chat/channels/{id}/messages, the agent's message ---".to_owned(),
            spoken.to_string(),
            format!(
                "--- audited: {} / {} / ok={} ---",
                runs[0].tool, runs[0].effect, runs[0].ok
            ),
        ],
    );
}

/// The read hands back the addresses every write needs — the block ids, their
/// kinds, their sections and whether each can be rewritten at all.
#[tokio::test]
async fn a_read_hands_back_the_blocks_a_write_will_be_addressed_to() {
    let h = harness("agent-a23-read").await;
    a_document(&h, "Terms of engagement").await;

    let (status, body) = run(&h, "doc_read", json!({})).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let result = &body["result"];
    assert_eq!(result["kind"], json!("docRead"));
    assert_eq!(result["document"]["name"], json!("Terms of engagement"));
    assert_eq!(result["document"]["blocks"], json!(6));
    assert_eq!(result["truncated"], json!(false));

    let read = result["blocks"].as_array().unwrap();
    assert_eq!(read.len(), 6, "the nested paragraph is a block too");
    assert_eq!(read[0]["block"], json!("b1"));
    assert_eq!(read[0]["kind"], json!("heading"));
    assert_eq!(read[0]["level"], json!(1));
    assert_eq!(
        read[2]["text"],
        json!("Invoices are due within 30 days of issue.")
    );
    assert_eq!(read[2]["section"], json!("Payment terms"));
    assert_eq!(read[2]["rewritable"], json!(true));
    // The nested paragraph reports its depth, so a section drafted after it is
    // not silently put at the top level.
    assert_eq!(read[4]["block"], json!("b5"));
    assert_eq!(read[4]["depth"], json!(1));
    // The table's words are readable and its structure is not rewritable —
    // said in the read, so a rewrite is never proposed for it.
    assert_eq!(read[5]["block"], json!("b6"));
    assert_eq!(read[5]["text"], json!("Region"));
    assert_eq!(read[5]["rewritable"], json!(false));

    // A window is a window, and says so.
    let (status, body) = run(&h, "doc_read", json!({ "from": "b3", "blocks": 2 })).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["result"]["from"], json!(3));
    assert_eq!(body["result"]["blocks"].as_array().unwrap().len(), 2);
    assert_eq!(body["result"]["truncated"], json!(true));

    // A block that is not in the document is a refusal, not an empty read.
    let (status, body) = run(&h, "doc_read", json!({ "from": "b99" })).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
    assert!(why(&body).contains("b99"), "{body}");
}

// ---- the writes ----------------------------------------------------------------

/// **A section waits for a tap, and lands as a version of the same document.**
///
/// The whole of ADR 0047 §3 for this product: the model asked for a write, the
/// room got a proposal and no change, and the change happened on the asker's own
/// approval — leaving every other block exactly as it was.
#[tokio::test]
async fn a_section_is_proposed_then_written_as_a_new_version_of_the_same_document() {
    let h = harness("agent-a23-draft").await;
    let doc = a_document(&h, "Terms of engagement").await;
    let (base, _seen) = scripted_model(vec![wants(
        "doc_draft_section",
        json!({
            "after": "b3",
            "blocks": [
                { "kind": "heading", "level": 2, "text": "Late payment" },
                { "kind": "paragraph", "text": "Interest accrues at 8% above base rate." }
            ]
        }),
        "I will add a section on late payment.",
    )])
    .await;
    use_model(&h, &base).await;
    let agent = the_docs_agent(&h).await;
    let channel = a_room_with(&h, "the contract", &agent).await;

    let spoken = ask_in_room(&h, &channel, "@docs add a section on late payment").await;
    let proposal = spoken["proposal"]["id"]
        .as_str()
        .expect("a write is proposed, never run")
        .to_owned();
    assert_eq!(spoken["proposal"]["tool"], json!("doc_draft_section"));
    // Nothing has happened yet: one version, and the document as it was.
    assert_eq!(h.acc.drive_versions(&doc).await.unwrap().len(), 1);
    assert_eq!(stored(&h, &doc).await, blocks());

    let decided = approve(&h, &proposal).await;
    let result = &decided["result"]["result"];
    assert_eq!(result["kind"], json!("docDraftSection"));
    assert_eq!(result["after"]["block"], json!("b3"));
    assert_eq!(result["added"][0]["kind"], json!("heading"));
    assert_eq!(
        result["added"][1]["text"],
        json!("Interest accrues at 8% above base rate.")
    );
    assert_eq!(result["versionNo"], json!(2));
    let added: Vec<String> = result["added"]
        .as_array()
        .unwrap()
        .iter()
        .map(|block| block["block"].as_str().unwrap().to_owned())
        .collect();

    // The document, as Drive now holds it: the new blocks after b3, at the top
    // level, and everything that was there still there and in order.
    let after = stored(&h, &doc).await;
    let ids: Vec<&str> = after
        .as_array()
        .unwrap()
        .iter()
        .map(|block| block["id"].as_str().unwrap())
        .collect();
    assert_eq!(
        ids,
        [
            "b1",
            "b2",
            "b3",
            added[0].as_str(),
            added[1].as_str(),
            "b4",
            "b6"
        ]
    );
    assert_eq!(after[3]["type"], json!("heading"));
    assert_eq!(after[3]["props"]["level"], json!(2));
    assert_eq!(
        after[4]["content"][0]["text"],
        json!("Interest accrues at 8% above base rate.")
    );
    // Untouched: the bold run inside the sentence above it, the nested
    // paragraph, and the table.
    assert_eq!(after[2]["content"][1]["styles"], json!({"bold": true}));
    assert_eq!(after[5]["children"][0]["id"], json!("b5"));
    assert_eq!(after[6]["content"]["type"], json!("tableContent"));
    // …and the old document is still in the history, so the change is
    // reversible the same way anybody else's is.
    assert_eq!(h.acc.drive_versions(&doc).await.unwrap().len(), 2);

    // Audited as a write.
    let runs = h.acc.agent_tool_runs(50).await.unwrap();
    assert_eq!(runs.len(), 1, "{runs:?}");
    assert_eq!(runs[0].tool, "doc_draft_section");
    assert_eq!(runs[0].effect, "write");
    let record = h.acc.agent_records().await.unwrap();
    assert_eq!(record.get(agent.as_str()).unwrap().actions, 1);
}

/// A rewrite replaces the words of a block and nothing else about it — and a
/// block whose content is a structure is refused **by name, before anything is
/// applied**.
#[tokio::test]
async fn a_rewrite_replaces_the_words_and_refuses_a_table_by_name() {
    let h = harness("agent-a23-rewrite").await;
    let doc = a_document(&h, "Terms of engagement").await;

    let (status, body) = run(
        &h,
        "doc_rewrite",
        json!({ "blocks": [
            { "block": "b3", "text": "Invoices are due within 14 days of issue." }
        ]}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let result = &body["result"];
    assert_eq!(result["kind"], json!("docRewrite"));
    assert_eq!(result["changed"], json!(1));
    assert_eq!(
        result["blocks"][0]["was"],
        json!("Invoices are due within 30 days of issue.")
    );
    assert_eq!(
        result["blocks"][0]["now"],
        json!("Invoices are due within 14 days of issue.")
    );
    assert_eq!(result["versionNo"], json!(2));

    let after = stored(&h, &doc).await;
    assert_eq!(after[2]["type"], json!("paragraph"), "the kind survives");
    assert_eq!(after[2]["props"]["textAlignment"], json!("left"));
    assert_eq!(
        after[2]["content"],
        json!([{"type": "text", "text": "Invoices are due within 14 days of issue.", "styles": {}}])
    );
    // Every other block is exactly as it was, including the table this rewrite
    // did not name.
    assert_eq!(after[0], blocks()[0]);
    assert_eq!(after[3], blocks()[3]);
    assert_eq!(after[4], blocks()[4]);

    // The table is refused by name, and nothing is written on the way to that
    // refusal — not even the paragraph named beside it.
    let (status, body) = run(
        &h,
        "doc_rewrite",
        json!({ "blocks": [
            { "block": "b2", "text": "Terms of payment" },
            { "block": "b6", "text": "North, South" }
        ]}),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
    let detail = why(&body);
    assert!(detail.contains("b6"), "{detail}");
    assert!(detail.contains("table"), "{detail}");
    assert_eq!(
        stored(&h, &doc).await[1]["content"][0]["text"],
        json!("Payment terms"),
        "the block named beside the table must not have been written"
    );

    // A block that is not there, and one named twice, are refused too.
    for refused in [
        json!({ "blocks": [{ "block": "b99", "text": "x" }] }),
        json!({ "blocks": [
            { "block": "b3", "text": "one" },
            { "block": "b3", "text": "two" }
        ]}),
        json!({ "blocks": [] }),
    ] {
        let (status, body) = run(&h, "doc_rewrite", refused.clone()).await;
        assert_eq!(
            status,
            StatusCode::UNPROCESSABLE_ENTITY,
            "{refused}: {body}"
        );
    }

    // A rewrite that changes nothing writes no version at all: an approved tool
    // must not leave a version in somebody's history saying it did.
    let (status, body) = run(
        &h,
        "doc_rewrite",
        json!({ "blocks": [
            { "block": "b3", "text": "Invoices are due within 14 days of issue." }
        ]}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["result"]["changed"], json!(0));
    assert_eq!(body["result"]["reason"], json!("nothingToRewrite"));
    assert_eq!(body["result"]["versionNo"], Value::Null);
    assert_eq!(h.acc.drive_versions(&doc).await.unwrap().len(), 2);
}

/// **Translating a document is that same rewrite** — the whole document
/// proposed once, in the new language, with the figures carried across
/// unchanged and the structure untouched.
///
/// There is deliberately no second mechanism: a translate tool of its own would
/// be a second path to keep honest, and this is the one that actually edits the
/// document.
#[tokio::test]
async fn a_translation_is_one_proposal_over_the_blocks_that_were_read() {
    let h = harness("agent-a23-translate").await;
    let doc = a_document(&h, "Terms of engagement").await;
    let (base, seen) = scripted_model(vec![
        wants("doc_read", json!({}), "Let me read it first."),
        wants(
            "doc_rewrite",
            json!({ "blocks": [
                { "block": "b1", "text": "Conditions d’engagement" },
                { "block": "b2", "text": "Conditions de paiement" },
                { "block": "b3", "text": "Les factures sont payables sous 30 jours." },
                { "block": "b4", "text": "Les retards de paiement sont facturés mensuellement." },
                { "block": "b5", "text": "Voir l’annexe pour le taux." }
            ]}),
            "Here is the document in French — the table is left as it is.",
        ),
    ])
    .await;
    use_model(&h, &base).await;
    let agent = the_docs_agent(&h).await;
    let channel = a_room_with(&h, "the contract", &agent).await;

    let spoken = ask_in_room(&h, &channel, "@docs translate this into French").await;
    let proposal = spoken["proposal"]["id"]
        .as_str()
        .expect("a translation is proposed, never applied")
        .to_owned();
    assert_eq!(spoken["proposal"]["tool"], json!("doc_rewrite"));
    // The model read the document before proposing: the ids it rewrote are the
    // ones the read handed it, not ids it invented.
    let read_back = shown(&seen, 1);
    assert!(read_back.contains("\"kind\":\"docRead\""), "{read_back}");
    assert!(read_back.contains("\"block\":\"b5\""), "{read_back}");
    assert_eq!(stored(&h, &doc).await, blocks(), "nothing yet");

    let decided = approve(&h, &proposal).await;
    let result = &decided["result"]["result"];
    assert_eq!(result["changed"], json!(5));
    assert_eq!(result["versionNo"], json!(2));

    let after = stored(&h, &doc).await;
    assert_eq!(
        after[0]["content"][0]["text"],
        json!("Conditions d’engagement")
    );
    assert_eq!(
        after[2]["content"][0]["text"],
        json!("Les factures sont payables sous 30 jours.")
    );
    // The nested paragraph is translated where it sits, not lifted out of the
    // list item it belongs to.
    assert_eq!(after[3]["children"][0]["id"], json!("b5"));
    assert_eq!(
        after[3]["children"][0]["content"][0]["text"],
        json!("Voir l’annexe pour le taux.")
    );
    // Structure survives a translation: the heading is still a heading at its
    // level, the list item is still a list item, and the table is untouched.
    assert_eq!(after[0]["type"], json!("heading"));
    assert_eq!(after[0]["props"]["level"], json!(1));
    assert_eq!(after[3]["type"], json!("bulletListItem"));
    assert_eq!(after[4], blocks()[4]);
    // …and the English is still in the history.
    assert_eq!(h.acc.drive_versions(&doc).await.unwrap().len(), 2);
    let record = h.acc.agent_records().await.unwrap();
    assert_eq!(record.get(agent.as_str()).unwrap().actions, 1);
}

/// A draft is refused whole when any part of it is wrong — a kind the editor
/// does not have, an `after` naming no block, or nothing to add at all.
#[tokio::test]
async fn a_draft_is_refused_whole_rather_than_applied_in_part() {
    let h = harness("agent-a23-draft-refuse").await;
    let doc = a_document(&h, "Terms of engagement").await;

    for refused in [
        json!({ "blocks": [{ "kind": "table", "text": "a grid" }] }),
        json!({ "blocks": [{ "kind": "paragraph" }] }),
        json!({ "blocks": [] }),
        json!({}),
        json!({ "after": "b99", "blocks": [{ "kind": "paragraph", "text": "x" }] }),
    ] {
        let (status, body) = run(&h, "doc_draft_section", refused.clone()).await;
        assert_eq!(
            status,
            StatusCode::UNPROCESSABLE_ENTITY,
            "{refused}: {body}"
        );
    }
    // Nothing was written on the way to any of those refusals.
    assert_eq!(stored(&h, &doc).await, blocks());
    assert_eq!(h.acc.drive_versions(&doc).await.unwrap().len(), 1);

    // …and with nothing named, a draft lands at the end of the document.
    let (status, body) = run(
        &h,
        "doc_draft_section",
        json!({ "blocks": [{ "kind": "paragraph", "text": "Signed in Brussels." }] }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["result"]["after"], Value::Null);
    let after = stored(&h, &doc).await;
    let last = after.as_array().unwrap().last().unwrap();
    assert_eq!(last["content"][0]["text"], json!("Signed in Brussels."));
    assert_eq!(after.as_array().unwrap().len(), 6);
}

// ---- isolation -------------------------------------------------------------------

/// **A document the asker could not open is not one the agent can name** —
/// across a tenant boundary and across a colleague's private Drive alike.
///
/// The refusal is the same one an unknown name gets, and it never says the
/// document exists: an agent that answered differently for a real file would be
/// a way to discover somebody else's documents by asking about them.
#[tokio::test]
async fn a_document_of_another_tenant_or_another_person_cannot_be_named() {
    let h = harness("agent-a23-isolation").await;
    // A second tenant on the same database, with a document of its own.
    let other = common::harness_on(h.store.clone(), "agent-a23-stranger").await;
    a_document_for(&other.acc, "Their secret strategy").await;
    // …and a colleague in the SAME tenant, with a private one.
    let colleague = h.ts.create_user("ben@a23-isolation.test").await.unwrap();
    let theirs = h.store.for_account(h.tenant.clone(), colleague.clone());
    a_document_for(&theirs, "Bens private appraisal").await;

    // Our own, so the refusals below are about reach and not about emptiness.
    a_document(&h, "Terms of engagement").await;

    for stranger in ["Their secret strategy", "Bens private appraisal"] {
        for (tool, args) in [
            ("doc_read", json!({ "document": stranger })),
            (
                "doc_answer",
                json!({ "document": stranger, "question": "anything" }),
            ),
            (
                "doc_draft_section",
                json!({ "document": stranger, "blocks": [{ "kind": "paragraph", "text": "x" }] }),
            ),
            (
                "doc_rewrite",
                json!({ "document": stranger, "blocks": [{ "block": "b1", "text": "x" }] }),
            ),
        ] {
            let (status, body) = run(&h, tool, args).await;
            assert_eq!(
                status,
                StatusCode::UNPROCESSABLE_ENTITY,
                "{tool} reached {stranger}: {body}"
            );
            assert!(
                why(&body).starts_with("no document of yours is called"),
                "{tool}/{stranger}: {body}"
            );
        }
    }

    // Neither document gained a version out of any of that.
    for (owner, name) in [
        (&other.acc, "Their secret strategy"),
        (&theirs, "Bens private appraisal"),
    ] {
        let found = owner.drive_docs(10).await.unwrap();
        let node = found
            .iter()
            .find(|node| node.name == name)
            .unwrap_or_else(|| panic!("{name} is gone"));
        assert_eq!(owner.drive_versions(&node.id).await.unwrap().len(), 1);
    }
}

/// The same, stated as the refusal's own shape: a name nothing matches lists
/// the caller's **own** documents and no one else's.
#[tokio::test]
async fn a_refusal_lists_only_the_askers_own_documents() {
    let h = harness("agent-a23-refusal").await;
    let other = common::harness_on(h.store.clone(), "agent-a23-refusal-stranger").await;
    a_document_for(&other.acc, "Their secret strategy").await;
    a_document(&h, "Terms of engagement").await;

    let (status, body) = run(&h, "doc_read", json!({ "document": "secret strategy" })).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
    let detail = why(&body);
    assert!(detail.contains("Terms of engagement"), "{detail}");
    assert!(
        !detail.contains("Their secret strategy"),
        "the refusal must not disclose another tenant's file: {detail}"
    );
}
