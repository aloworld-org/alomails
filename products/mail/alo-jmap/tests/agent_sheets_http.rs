//! **The Sheet agent, end to end** (A2.2) — the four sentences ADR 0034 and ADR
//! 0047 leave a spreadsheet agent to prove, each asked the way a person asks it
//! and answered the way a person reads it:
//!
//! - `@sheets what did the North region bring in in January?` — answered from
//!   the cells of the workbook, **with the address of each figure**, and **no
//!   button in between**;
//! - `@sheets what does the total in B5 do?` — a formula explained from what it
//!   actually points at and what those cells hold now;
//! - a formula the agent proposes **waits for a tap**, and when it lands it
//!   lands as a new Drive version of the same workbook, with everything else in
//!   the document — the other cells, the other tab, the styles — untouched;
//! - a column tidy changes **how the column was typed and nothing about what it
//!   means**, and never a cell holding a formula.
//!
//! And the two isolation sentences the wave holds every agent to: a workbook of
//! another tenant's cannot be named, and neither can a colleague's.
//!
//! Everything goes through the product's own path: the tenant's agents are the
//! ones `GET /chat/agents` seeds (A1.5), the room is made over HTTP, the agent
//! joins it over HTTP, and the question is an ordinary chat message. The
//! spreadsheet is a real Drive node whose blob is the same JSON snapshot
//! `web/src/drive/SheetEditor.tsx` writes.
//!
//! **No live model is ever called**, here or anywhere in this workspace's tests
//! (the loop's standing rail): the tenant's AI backend is the scripted local
//! socket in `common::model`. The assertions below are about the bytes the model
//! was *shown*, which is where an answer read off the grid and a plausible guess
//! differ.
//!
//! Run the transcript with
//! `cargo nextest run -p alo-jmap --test agent_sheets_http --no-capture`.

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

/// The id of the tenant's own Sheet agent, out of the set a first look at
/// `GET /chat/agents` seeds (A1.5). Nothing here registers a handle: an agent
/// this test could not find is an agent a person could not find either.
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
    println!("\n===== A2.2 TRANSCRIPT: {title} =====");
    for line in lines {
        println!("{line}");
    }
    println!("===== end: {title} =====\n");
}

// ---- the spreadsheet under test ----------------------------------------------

/// A workbook the way a real one is: a header row, two regions of figures, a
/// total that is a formula, a second column somebody pasted in as text with
/// spaces around it, a styled-but-empty cell, and a second tab. Every one of
/// those is something the agent must handle rather than trip on.
fn snapshot() -> Value {
    json!({
        "id": "wb-q1",
        "name": "Q1 figures",
        "appVersion": "0.25.1",
        "locale": "enUS",
        "sheetOrder": ["sheet-1", "sheet-2"],
        "sheets": {
            "sheet-1": {
                "id": "sheet-1",
                "name": "Revenue",
                "rowCount": 100,
                "columnCount": 26,
                "cellData": {
                    "0": {
                        "0": {"v": "Region", "t": 1},
                        "1": {"v": "January", "t": 1},
                        "2": {"v": "February", "t": 1}
                    },
                    "1": {
                        "0": {"v": "North", "t": 1},
                        "1": {"v": 1200, "t": 2},
                        "2": {"v": " 1300 ", "t": 1, "s": "style-3"}
                    },
                    "2": {
                        "0": {"v": "South", "t": 1},
                        "1": {"v": 900, "t": 2},
                        "2": {"v": "1100", "t": 1}
                    },
                    "3": {
                        "0": {"v": "Total", "t": 1},
                        "1": {"f": "=SUM(B2:B3)", "v": 2100, "t": 2}
                    },
                    "4": { "0": {"s": "style-7"} }
                }
            },
            "sheet-2": {
                "id": "sheet-2",
                "name": "Notes",
                "cellData": { "0": { "0": {"v": "Figures from the ledger.", "t": 1} } }
            }
        }
    })
}

/// Writes that workbook into the caller's own Drive as a `sheet` node, exactly
/// as the editor's own save does — a blob, then a node pointing at it.
async fn a_spreadsheet(h: &Harness, name: &str) -> DriveNodeId {
    a_spreadsheet_for(&h.acc, name).await
}

async fn a_spreadsheet_for(acc: &alo_store::AccountStore, name: &str) -> DriveNodeId {
    let bytes = serde_json::to_vec(&snapshot()).unwrap();
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
            kind: Some("sheet".to_owned()),
            ..NewDriveFile::default()
        },
    )
    .await
    .unwrap()
}

/// The workbook as it stands in Drive right now, read back through the store the
/// way the editor would read it.
async fn stored(h: &Harness, node: &DriveNodeId) -> Value {
    let node = h.acc.drive_node(node).await.unwrap().unwrap();
    let blob = alo_store::BlobId::new(node.blob_id.unwrap());
    let bytes = h.acc.blob_bytes_for_send(&blob).await.unwrap();
    serde_json::from_slice(&bytes).unwrap()
}

// ---- the question, on the wire -----------------------------------------------

/// **The wave's question, end to end.** What lands in the room is the answer,
/// grounded in the cells of the workbook and carrying their addresses — and
/// there is no button in between, because asking what a figure is does not
/// change anything.
#[tokio::test]
async fn the_sheet_agent_answers_from_the_cells_and_cites_them_with_no_button_in_between() {
    let h = harness("agent-a22-answer").await;
    let sheet = a_spreadsheet(&h, "Q1 figures").await;
    const ANSWER: &str =
        "North brought in 1 200 in January (B2 of Revenue), against 900 for South in B3 [1].";
    let (base, seen) = scripted_model(vec![
        wants(
            "sheet_answer",
            json!({ "question": "North region January" }),
            "Let me look in the sheet.",
        ),
        says(ANSWER),
    ])
    .await;
    use_model(&h, &base).await;
    let agent = the_sheet_agent(&h).await;
    let channel = a_room_with(&h, "the figures", &agent).await;

    const QUESTION: &str = "@sheets what did the North region bring in in January?";
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
    // the room. Asking what a cell holds is a lookup, not a change.
    for message in &room {
        assert_eq!(
            message["proposal"],
            Value::Null,
            "asking what a figure is must never produce a proposal: {message}"
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
        system.contains("- sheet_answer:"),
        "the Sheet agent is offered its own reading tool: {system}"
    );
    assert!(
        !system.contains("- stock_answer:") && !system.contains("- site_answer:"),
        "and only its own product's tools (A1.2): {system}"
    );
    // Its grounding is empty on purpose (A1.3): Sheets reaches its records
    // through the tool, so the first call carries the question and no snippets.
    assert!(
        first.contains("Sources:\n\n") || first.trim_end().ends_with("Sources:"),
        "the question is not answered from a search snippet: {first}"
    );
    // **The cells, with their addresses.** This is the assertion the item is
    // named after: an answer a person can check against their own grid.
    assert!(
        second.contains("tool result \"sheet_answer\""),
        "the tool's own result must be among the sources: {second}"
    );
    assert!(second.contains("\"kind\":\"sheetAnswer\""), "{second}");
    assert!(second.contains("\"cell\":\"A2\""), "{second}");
    assert!(second.contains("\"cell\":\"B2\""), "{second}");
    assert!(
        second.contains("\"header\":\"January\""),
        "a figure comes back captioned by the column it is under: {second}"
    );
    assert!(
        second.contains("\"text\":\"1200\""),
        "the figure itself, as the cell stores it: {second}"
    );
    // The row that did not match is not in the answer, and neither is the other
    // tab's prose: a search result is the rows that matched.
    assert!(!second.contains("South"), "{second}");
    assert!(!second.contains("Figures from the ledger"), "{second}");

    // Audited as a read — the agent's, the room's, and successful.
    let runs = h.acc.agent_tool_runs(50).await.unwrap();
    assert_eq!(runs.len(), 1, "{runs:?}");
    assert_eq!(runs[0].tool, "sheet_answer");
    assert_eq!(runs[0].effect, "read");
    assert!(runs[0].ok);
    let record = h.acc.agent_records().await.unwrap();
    let record = record.get(agent.as_str()).unwrap();
    assert_eq!(record.reads, 1);
    assert_eq!(record.answers, 1);
    assert_eq!(record.actions, 0);

    // Nothing was written: asking a question about a spreadsheet leaves it on
    // the version it was on.
    assert_eq!(h.acc.drive_versions(&sheet).await.unwrap().len(), 1);
    assert_eq!(stored(&h, &sheet).await, snapshot());

    transcript(
        QUESTION,
        &[
            format!("POST /chat/channels/{channel}/messages"),
            format!("     {}", json!({ "body": QUESTION })),
            "--- what the model was shown (call 1 of 2, user turn) ---".to_owned(),
            first,
            "--- what the model replied (call 1) ---".to_owned(),
            wants(
                "sheet_answer",
                json!({ "question": "North region January" }),
                "Let me look in the sheet.",
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

/// A formula explained from what it actually reads — the range, and what those
/// cells hold right now — rather than from the text in the formula bar, which
/// the user can already see.
#[tokio::test]
async fn a_formula_is_explained_from_the_cells_it_reads() {
    let h = harness("agent-a22-formula").await;
    a_spreadsheet(&h, "Q1 figures").await;
    const ANSWER: &str =
        "B4 adds January's two regions — 1 200 in B2 and 900 in B3 — with SUM [1].";
    let (base, seen) = scripted_model(vec![
        wants(
            "sheet_formula_explain",
            json!({ "cell": "B4" }),
            "Let me read that cell.",
        ),
        says(ANSWER),
    ])
    .await;
    use_model(&h, &base).await;
    let agent = the_sheet_agent(&h).await;
    let channel = a_room_with(&h, "the formula", &agent).await;

    let spoken = ask_in_room(&h, &channel, "@sheets what does the total in B4 do?").await;
    assert_eq!(spoken["body"], json!(ANSWER));
    let second = shown(&seen, 1);
    assert!(second.contains("\"formula\":\"=SUM(B2:B3)\""), "{second}");
    assert!(second.contains("\"functions\":[\"SUM\"]"), "{second}");
    assert!(second.contains("\"ref\":\"B2:B3\""), "{second}");
    // What those cells hold *now* — the half a formula's own text cannot say.
    assert!(second.contains("\"cell\":\"B2\""), "{second}");
    assert!(second.contains("\"text\":\"1200\""), "{second}");
    assert!(second.contains("\"text\":\"900\""), "{second}");
    // Still a read, still no button.
    for message in messages(&h, &channel).await {
        assert_eq!(message["proposal"], Value::Null, "{message}");
    }
    let record = h.acc.agent_records().await.unwrap();
    assert_eq!(record.get(agent.as_str()).unwrap().actions, 0);
}

/// A cell that is not a formula is a different answer, and is never dressed up
/// as one.
#[tokio::test]
async fn a_cell_that_holds_a_value_is_not_reported_as_a_formula() {
    let h = harness("agent-a22-notformula").await;
    a_spreadsheet(&h, "Q1 figures").await;

    let (status, body) = run(&h, "sheet_formula_explain", json!({ "cell": "B2" })).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let result = &body["result"];
    assert_eq!(result["hasFormula"], json!(false));
    assert_eq!(result["reason"], json!("notAFormula"));
    assert_eq!(result["value"], json!("1200"));
    assert_eq!(result["references"], json!([]));

    // …and an empty cell is a third answer, not the same one.
    let (status, body) = run(&h, "sheet_formula_explain", json!({ "cell": "Z40" })).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["result"]["reason"], json!("emptyCell"));
    assert_eq!(body["result"]["hasFormula"], json!(false));
}

/// One approved tool run over the ordinary approval route — `POST
/// /ai/agent/execute`, the same path the command palette's button takes, which
/// is an approval the caller gave with their own session.
///
/// The tests that use it are about **arguments and refusals**, which a chat
/// turn cannot vary as finely as they need; the chat path itself is what the
/// room exercises above.
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

// ---- the writes ----------------------------------------------------------------

/// **A formula waits for a tap, and lands as a version of the same document.**
///
/// The whole of ADR 0047 §3 for this product: the model asked for a write, the
/// room got a proposal and no change, and the change happened on the asker's own
/// approval — leaving every other cell, the other tab and the styles exactly as
/// they were.
#[tokio::test]
async fn a_formula_is_proposed_then_written_as_a_new_version_of_the_same_workbook() {
    let h = harness("agent-a22-write").await;
    let sheet = a_spreadsheet(&h, "Q1 figures").await;
    let (base, _seen) = scripted_model(vec![wants(
        "sheet_write_formula",
        json!({ "cells": [{ "cell": "C4", "formula": "=SUM(C2:C3)" }] }),
        "I will total February in C4.",
    )])
    .await;
    use_model(&h, &base).await;
    let agent = the_sheet_agent(&h).await;
    let channel = a_room_with(&h, "the totals", &agent).await;

    let spoken = ask_in_room(&h, &channel, "@sheets total February under the column").await;
    let proposal = spoken["proposal"]["id"]
        .as_str()
        .expect("a write is proposed, never run")
        .to_owned();
    assert_eq!(spoken["proposal"]["tool"], json!("sheet_write_formula"));
    // Nothing has happened yet: one version, and the cell is still empty.
    assert_eq!(h.acc.drive_versions(&sheet).await.unwrap().len(), 1);
    assert_eq!(stored(&h, &sheet).await, snapshot());

    let decided = approve(&h, &proposal).await;
    let result = &decided["result"]["result"];
    assert_eq!(result["kind"], json!("sheetWriteFormula"));
    assert_eq!(result["written"][0]["cell"], json!("C4"));
    assert_eq!(result["written"][0]["formula"], json!("=SUM(C2:C3)"));
    assert_eq!(result["recalculates"], json!("onOpen"));
    assert_eq!(result["versionNo"], json!(2));

    // The document, as Drive now holds it.
    let after = stored(&h, &sheet).await;
    assert_eq!(
        after["sheets"]["sheet-1"]["cellData"]["3"]["2"]["f"],
        json!("=SUM(C2:C3)")
    );
    // Everything else survives a write the user approved for one cell: the
    // other cells, the styles on them, the second tab, and the row counts.
    assert_eq!(
        after["sheets"]["sheet-1"]["cellData"]["1"]["1"]["v"],
        json!(1200)
    );
    assert_eq!(
        after["sheets"]["sheet-1"]["cellData"]["1"]["2"]["s"],
        json!("style-3")
    );
    assert_eq!(after["sheets"]["sheet-1"]["rowCount"], json!(100));
    assert_eq!(
        after["sheets"]["sheet-2"]["cellData"]["0"]["0"]["v"],
        json!("Figures from the ledger.")
    );
    assert_eq!(after["id"], json!("wb-q1"));
    // …and the old document is still in the history, so the change is
    // reversible the same way anybody else's is.
    assert_eq!(h.acc.drive_versions(&sheet).await.unwrap().len(), 2);

    // Audited as a write.
    let runs = h.acc.agent_tool_runs(50).await.unwrap();
    assert_eq!(runs.len(), 1, "{runs:?}");
    assert_eq!(runs[0].tool, "sheet_write_formula");
    assert_eq!(runs[0].effect, "write");
    let record = h.acc.agent_records().await.unwrap();
    assert_eq!(record.get(agent.as_str()).unwrap().actions, 1);
}

/// The two refusals a formula write is built on: it writes calculations and
/// never data, and it does not quietly overwrite a figure.
#[tokio::test]
async fn a_write_refuses_a_fact_and_refuses_to_overwrite_a_figure_unasked() {
    let h = harness("agent-a22-refuse").await;
    let sheet = a_spreadsheet(&h, "Q1 figures").await;

    // A value dressed as a write is refused, and the refusal says why.
    let (status, body) = run(
        &h,
        "sheet_write_formula",
        json!({ "cells": [{ "cell": "D2", "formula": "4200" }] }),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
    assert!(why(&body).contains("starts with ="), "{body}");

    // A cell already holding a figure is refused **by name** rather than
    // overwritten, and the refusal names every one of them at once.
    let (status, body) = run(
        &h,
        "sheet_write_formula",
        json!({ "cells": [
            { "cell": "B2", "formula": "=1+1" },
            { "cell": "D9", "formula": "=2+2" },
            { "cell": "B3", "formula": "=3+3" }
        ]}),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
    let detail = why(&body);
    assert!(detail.contains("B2"), "{detail}");
    assert!(detail.contains("B3"), "{detail}");
    assert!(
        !detail.contains("D9"),
        "an empty cell is not occupied: {detail}"
    );
    // Nothing was written on the way to that refusal — not even the empty cell.
    assert_eq!(stored(&h, &sheet).await, snapshot());
    assert_eq!(h.acc.drive_versions(&sheet).await.unwrap().len(), 1);

    // Said explicitly, it goes through, and reports what it replaced.
    let (status, body) = run(
        &h,
        "sheet_write_formula",
        json!({ "replace": true, "cells": [{ "cell": "B2", "formula": "=1000+200" }] }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["result"]["written"][0]["replaced"], json!("1200"));
    assert_eq!(
        stored(&h, &sheet).await["sheets"]["sheet-1"]["cellData"]["1"]["1"]["f"],
        json!("=1000+200")
    );

    // A cell named twice is a refusal too: which formula it should hold was
    // never stated.
    let (status, body) = run(
        &h,
        "sheet_write_formula",
        json!({ "cells": [
            { "cell": "E1", "formula": "=1" },
            { "cell": "e1", "formula": "=2" }
        ]}),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
    assert!(
        why(&body).contains("named twice"),
        "the second formula must not silently win: {body}"
    );
}

/// **A tidy changes the typing and nothing else** — and leaves every formula
/// cell alone, because a formula's text is an answer the sheet computed.
#[tokio::test]
async fn a_column_tidy_changes_how_it_was_typed_and_never_what_it_means() {
    let h = harness("agent-a22-tidy").await;
    let sheet = a_spreadsheet(&h, "Q1 figures").await;

    let (status, body) = run(&h, "sheet_clean_column", json!({ "column": "C" })).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let result = &body["result"];
    assert_eq!(result["kind"], json!("sheetCleanColumn"));
    assert_eq!(result["column"], json!("C"));
    assert_eq!(result["header"], json!("February"));
    // It started under the header, not on it: the label is not data to tidy.
    assert_eq!(result["from"], json!("C2"));
    assert_eq!(result["changed"], json!(2));
    assert_eq!(result["cells"][0]["cell"], json!("C2"));
    assert_eq!(result["cells"][0]["was"], json!(" 1300 "));
    assert_eq!(result["cells"][0]["now"], json!(1300));
    assert_eq!(
        result["cells"][0]["did"],
        json!(["trimmed", "storedAsNumber"])
    );
    assert_eq!(result["cells"][1]["now"], json!(1100));

    let after = stored(&h, &sheet).await;
    // The tidied cells are numbers now, typed as numbers, with their styles.
    assert_eq!(
        after["sheets"]["sheet-1"]["cellData"]["1"]["2"]["v"],
        json!(1300)
    );
    assert_eq!(
        after["sheets"]["sheet-1"]["cellData"]["1"]["2"]["t"],
        json!(2)
    );
    assert_eq!(
        after["sheets"]["sheet-1"]["cellData"]["1"]["2"]["s"],
        json!("style-3"),
        "a tidy must not cost the column its formatting"
    );
    // The header is untouched, and so is every other column.
    assert_eq!(
        after["sheets"]["sheet-1"]["cellData"]["0"]["2"]["v"],
        json!("February")
    );
    assert_eq!(
        after["sheets"]["sheet-1"]["cellData"]["1"]["0"]["v"],
        json!("North")
    );

    // The column of totals is never tidied: B4 is a formula, and rewriting its
    // cached answer as a value would freeze the calculation.
    let (status, body) = run(&h, "sheet_clean_column", json!({ "column": "B" })).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let result = &body["result"];
    assert_eq!(result["skippedFormulas"], json!(1));
    assert_eq!(result["changed"], json!(0));
    assert_eq!(result["reason"], json!("nothingToTidy"));
    // Nothing to tidy wrote no version at all: an approved tool that changed
    // nothing must not leave a version saying it did.
    assert_eq!(result["versionNo"], Value::Null);
    assert_eq!(h.acc.drive_versions(&sheet).await.unwrap().len(), 2);
    assert_eq!(
        stored(&h, &sheet).await["sheets"]["sheet-1"]["cellData"]["3"]["1"]["f"],
        json!("=SUM(B2:B3)")
    );
}

// ---- isolation -------------------------------------------------------------------

/// **A spreadsheet the asker could not open is not one the agent can name** —
/// across a tenant boundary and across a colleague's private Drive alike.
///
/// The refusal is the same one an unknown name gets, and it never says the
/// workbook exists: an agent that answered differently for a real file would be
/// a way to discover somebody else's documents by asking about them.
#[tokio::test]
async fn a_workbook_of_another_tenant_or_another_person_cannot_be_named() {
    let h = harness("agent-a22-isolation").await;
    // A second tenant on the same database, with a spreadsheet of its own.
    let other = common::harness_on(h.store.clone(), "agent-a22-stranger").await;
    a_spreadsheet_for(&other.acc, "Their secret budget").await;
    // …and a colleague in the SAME tenant, with a private one.
    let colleague = h.ts.create_user("ben@a22-isolation.test").await.unwrap();
    let theirs = h.store.for_account(h.tenant.clone(), colleague.clone());
    a_spreadsheet_for(&theirs, "Bens private pay review").await;

    // Our own, so the refusals below are about reach and not about emptiness.
    a_spreadsheet(&h, "Q1 figures").await;

    for stranger in ["Their secret budget", "Bens private pay review"] {
        for tool in [
            "sheet_read",
            "sheet_answer",
            "sheet_formula_explain",
            "sheet_clean_column",
        ] {
            let (status, body) = run(
                &h,
                tool,
                json!({
                    "workbook": stranger,
                    "question": "anything",
                    "cell": "A1",
                    "column": "A"
                }),
            )
            .await;
            assert_eq!(
                status,
                StatusCode::UNPROCESSABLE_ENTITY,
                "{tool} reached {stranger}: {body}"
            );
            assert!(
                why(&body).starts_with("no spreadsheet of yours is called"),
                "{tool}/{stranger}: {body}"
            );
        }
        // …and the write, which must not create one either.
        let (status, body) = run(
            &h,
            "sheet_write_formula",
            json!({ "workbook": stranger, "cells": [{ "cell": "Z1", "formula": "=1" }] }),
        )
        .await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
    }

    // Neither workbook gained a version out of any of that.
    for (owner, name) in [
        (&other.acc, "Their secret budget"),
        (&theirs, "Bens private pay review"),
    ] {
        let found = owner.drive_sheets(10).await.unwrap();
        let node = found
            .iter()
            .find(|node| node.name == name)
            .unwrap_or_else(|| panic!("{name} is gone"));
        assert_eq!(owner.drive_versions(&node.id).await.unwrap().len(), 1);
    }
}

/// The same, stated as the refusal's own shape: a name nothing matches lists
/// the caller's **own** spreadsheets and no one else's.
#[tokio::test]
async fn a_refusal_lists_only_the_askers_own_spreadsheets() {
    let h = harness("agent-a22-refusal").await;
    let other = common::harness_on(h.store.clone(), "agent-a22-refusal-stranger").await;
    a_spreadsheet_for(&other.acc, "Their secret budget").await;
    a_spreadsheet(&h, "Q1 figures").await;

    let (status, body) = run(&h, "sheet_read", json!({ "workbook": "secret budget" })).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
    let detail = why(&body);
    assert!(detail.contains("Q1 figures"), "{detail}");
    assert!(
        !detail.contains("Their secret budget"),
        "the refusal must not disclose another tenant's file: {detail}"
    );
}
