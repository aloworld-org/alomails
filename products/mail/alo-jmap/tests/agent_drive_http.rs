//! **The Drive agent, end to end** (A2.5) — the three sentences the queue item
//! leaves a file agent to prove, each asked the way a person asks it:
//!
//! - `@drive what's in the handover note?` — **summarised from the file's own
//!   text**, read inside the turn with no button in between, and the model is
//!   shown the words that are actually in the file;
//! - `@drive what does the spreadsheet they sent say?` — the attachment of an
//!   email the user named by its subject, pulled out and read; and a PDF
//!   attachment **refused by name and by type** rather than described from its
//!   filename, which is the one failure that would make the agent a liar;
//! - `@drive rename this and file it under Contracts` — a rename and a move that
//!   **wait for a tap**, keep the file's extension, and change nothing inside
//!   the file or about who can read it.
//!
//! And the isolation sentence the wave holds every agent to: a file of another
//! tenant's cannot be named, and neither can a colleague's — the refusal is the
//! same one an unknown name gets, so no one learns a stranger's file exists by
//! asking about it.
//!
//! Everything goes through the product's own path: the tenant's agents are the
//! ones `GET /chat/agents` seeds (A1.5), the room is made over HTTP, the agent
//! joins it over HTTP, and the question is an ordinary chat message. The files
//! are real Drive nodes and the email is a real delivered message.
//!
//! **No live model is ever called**, here or anywhere in this workspace's tests
//! (the loop's standing rail): the tenant's AI backend is the scripted local
//! socket in `common::model`. The assertions below are about the bytes the model
//! was *shown*, which is where a summary read out of a file and a plausible
//! guess differ.
//!
//! Run the transcript with
//! `cargo nextest run -p alo-jmap --test agent_drive_http --no-capture`.

#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use std::time::{Duration, Instant};

use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use bytes::Bytes;
use serde_json::{Value, json};

use alo_store::{AccountStore, AgentProduct, DriveLocation, DriveNodeId, NewDriveFile};
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

/// The id of the tenant's own Drive agent, out of the set a first look at
/// `GET /chat/agents` seeds (A1.5).
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

/// What the model was shown on call `n` — the user turn, where the grounding and
/// the tool results live.
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

fn transcript(title: &str, lines: &[String]) {
    println!("\n===== A2.5 TRANSCRIPT: {title} =====");
    for line in lines {
        println!("{line}");
    }
    println!("===== end: {title} =====\n");
}

// ---- the files under test -----------------------------------------------------

/// A document the way the editor stores one: a block array with headings, prose
/// and a list.
fn blocks() -> Value {
    json!([
        {"id": "b1", "type": "heading", "props": {"level": 1},
         "content": [{"type": "text", "text": "Handover", "styles": {}}], "children": []},
        {"id": "b2", "type": "paragraph", "props": {},
         "content": [
            {"type": "text", "text": "Marta takes over the ", "styles": {}},
            {"type": "text", "text": "Delaunay", "styles": {"bold": true}},
            {"type": "text", "text": " account on 1 September.", "styles": {}}
         ], "children": []},
        {"id": "b3", "type": "bulletListItem", "props": {},
         "content": [{"type": "text", "text": "Renewal is due in November.", "styles": {}}],
         "children": []}
    ])
}

/// Writes that document into a Drive as a `doc` node, exactly as the editor's
/// own save does — a blob, then a node pointing at it.
async fn a_document_for(acc: &AccountStore, name: &str) -> DriveNodeId {
    let bytes = serde_json::to_vec(&blocks()).unwrap();
    a_file_for(acc, name, &bytes, "application/json", Some("doc"), None).await
}

/// Any file, of any kind, in a Drive — the general form the tests above build on.
async fn a_file_for(
    acc: &AccountStore,
    name: &str,
    bytes: &[u8],
    content_type: &str,
    kind: Option<&str>,
    parent: Option<&DriveNodeId>,
) -> DriveNodeId {
    let size = i64::try_from(bytes.len()).unwrap();
    let blob = acc
        .put_blob(Bytes::from(bytes.to_vec()), Some(content_type))
        .await
        .unwrap();
    acc.drive_create_file(
        &DriveLocation::Personal,
        parent,
        &NewDriveFile {
            name: name.to_owned(),
            blob_id: blob.as_str().to_owned(),
            size,
            content_type: Some(content_type.to_owned()),
            kind: kind.map(str::to_owned),
            ..NewDriveFile::default()
        },
    )
    .await
    .unwrap()
}

/// The name a Drive node has right now, read back through the store.
async fn name_of(h: &Harness, node: &DriveNodeId) -> String {
    h.acc.drive_node(node).await.unwrap().unwrap().name
}

/// The folder a Drive node sits in right now.
async fn parent_of(h: &Harness, node: &DriveNodeId) -> Option<String> {
    h.acc
        .drive_node(node)
        .await
        .unwrap()
        .unwrap()
        .parent_id
        .map(|id| id.as_str().to_owned())
}

/// An email with two attachments, delivered into the caller's inbox the way the
/// SMTP path delivers one.
async fn an_email_with_attachments(acc: &AccountStore) {
    let raw = concat!(
        "From: paula@delaunay.example\r\n",
        "To: me@example.test\r\n",
        "Subject: Q3 figures\r\n",
        "MIME-Version: 1.0\r\n",
        "Content-Type: multipart/mixed; boundary=\"sep\"\r\n",
        "\r\n",
        "--sep\r\n",
        "Content-Type: text/plain; charset=utf-8\r\n",
        "\r\n",
        "As promised, the numbers.\r\n",
        "--sep\r\n",
        "Content-Type: text/csv; charset=utf-8\r\n",
        "Content-Disposition: attachment; filename=\"q3.csv\"\r\n",
        "\r\n",
        "region,revenue\r\nBenelux,142000\r\nNordics,98000\r\n",
        "--sep\r\n",
        "Content-Type: application/pdf\r\n",
        "Content-Disposition: attachment; filename=\"board-pack.pdf\"\r\n",
        "Content-Transfer-Encoding: base64\r\n",
        "\r\n",
        "JVBERi0xLjQKJeLjz9MK\r\n",
        "--sep--\r\n",
    );
    acc.deliver(raw.as_bytes()).await.unwrap();
}

// ---- summarising a file, on the wire -------------------------------------------

/// **The item's first sentence, end to end.** The question is answered from what
/// the file actually says — the model is shown the document's own words — and
/// there is no button in between, because reading a file does not change it.
#[tokio::test]
async fn the_drive_agent_summarises_a_file_it_actually_read_with_no_button_in_between() {
    let h = harness("agent-a25-summary").await;
    a_document_for(&h.acc, "Handover note").await;
    const ANSWER: &str = "The handover note says Marta takes over the Delaunay account on 1 September, \
         and that the renewal is due in November [1].";
    let (base, seen) = scripted_model(vec![
        wants(
            "file_read",
            json!({ "file": "Handover note" }),
            "Let me read the note.",
        ),
        says(ANSWER),
    ])
    .await;
    use_model(&h, &base).await;
    let agent = the_drive_agent(&h).await;
    let channel = a_room_with(&h, "the handover", &agent).await;

    const QUESTION: &str = "@drive what's in the handover note?";
    let spoken = ask_in_room(&h, &channel, QUESTION).await;

    assert_eq!(spoken["body"], json!(ANSWER));
    assert_eq!(spoken["authorKind"], json!("agent"));
    assert_eq!(
        spoken["proposal"],
        Value::Null,
        "asking what a file says must not put a button in the room"
    );

    // The words the summary was written from were in front of the model — this
    // is the difference between a summary and a guess.
    let second = shown(&seen, 1);
    assert!(second.contains("driveFileText"), "{second}");
    assert!(
        second.contains("Marta takes over the Delaunay account on 1 September."),
        "the model was not shown the file's own text: {second}"
    );
    assert!(second.contains("Renewal is due in November."), "{second}");
    // …and no block id came with it: Drive reads files, Docs edits documents.
    assert!(
        !second.contains("\"b2\""),
        "a Drive read must hand back no address a write could use: {second}"
    );

    transcript(
        QUESTION,
        &[
            format!("POST /chat/channels/{channel}/messages"),
            format!("     {}", json!({ "body": QUESTION })),
            "--- what the model was shown (call 2 of 2, user turn) ---".to_owned(),
            second,
            "--- what the model replied (call 2) ---".to_owned(),
            says(ANSWER),
            "--- GET /chat/channels/{id}/messages, the agent's message ---".to_owned(),
            spoken.to_string(),
        ],
    );
}

/// What a read hands back, and what it refuses. A `.txt` file is decoded, a
/// document is flattened with its shape intact, and a picture is refused **by
/// name** — never summarised from its filename.
#[tokio::test]
async fn a_file_is_read_as_text_or_refused_by_name_and_never_guessed_at() {
    let h = harness("agent-a25-read").await;
    a_document_for(&h.acc, "Handover note").await;
    a_file_for(
        &h.acc,
        "shopping.txt",
        b"milk\nbread\n",
        "text/plain",
        None,
        None,
    )
    .await;
    // Mislabelled by whatever uploaded it, and still plainly a person's notes.
    a_file_for(
        &h.acc,
        "notes.md",
        b"# Notes\n\nCall Paula.\n",
        "application/octet-stream",
        None,
        None,
    )
    .await;
    a_file_for(
        &h.acc,
        "scan.png",
        b"\x89PNG\r\n\x1a\n",
        "image/png",
        None,
        None,
    )
    .await;
    // A .txt whose bytes are not UTF-8 at all: mislabelled the other way.
    a_file_for(
        &h.acc,
        "legacy.txt",
        b"\xff\xfe\x00bad",
        "text/plain",
        None,
        None,
    )
    .await;

    // A document, flattened: its shape survives, its block ids do not.
    let (status, body) = run(&h, "file_read", json!({ "file": "Handover note" })).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let result = &body["result"];
    assert_eq!(result["kind"], json!("driveFileText"));
    assert_eq!(result["file"]["name"], json!("Handover note"));
    assert_eq!(result["truncated"], json!(false));
    let text = result["text"].as_str().unwrap();
    assert!(text.contains("# Handover"), "{text}");
    assert!(
        text.contains("Marta takes over the Delaunay account on 1 September."),
        "the bold run is flattened into the sentence: {text}"
    );
    assert!(text.contains("- Renewal is due in November."), "{text}");
    assert!(!text.contains("b1"), "no address a write could use: {text}");

    // A plain file, decoded.
    let (status, body) = run(&h, "file_read", json!({ "file": "shopping.txt" })).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["result"]["text"], json!("milk\nbread\n"));
    assert_eq!(body["result"]["words"], json!(2));

    // Type says octet-stream, name says markdown: the name wins, because a .md
    // is still a person's notes whatever the uploader labelled it.
    let (status, body) = run(&h, "file_read", json!({ "file": "notes.md" })).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert!(
        body["result"]["text"]
            .as_str()
            .unwrap()
            .contains("Call Paula")
    );

    // A window is a window, and says so.
    let (status, body) = run(
        &h,
        "file_read",
        json!({ "file": "shopping.txt", "chars": 4 }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["result"]["text"], json!("milk"));
    assert_eq!(body["result"]["truncated"], json!(true));

    // …and the two refusals, each naming the file and what it is.
    for (file, what) in [("scan.png", ".png"), ("legacy.txt", ".txt")] {
        let (status, body) = run(&h, "file_read", json!({ "file": file })).await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{file}: {body}");
        let detail = why(&body);
        assert!(detail.contains(file), "{detail}");
        assert!(detail.contains(what), "{detail}");
        assert!(
            detail.contains("say so rather than describing what it might contain"),
            "{detail}"
        );
    }

    // A name nothing matches is a refusal, never the first file in the drive.
    let (status, body) = run(&h, "file_read", json!({ "file": "the other one" })).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
    assert!(
        why(&body).starts_with("no file of yours is called"),
        "{body}"
    );
    let (status, body) = run(&h, "file_read", json!({})).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
}

/// A spreadsheet is not prose, and the refusal says whose job it is — an answer
/// the model can pass on beats a failure it has to paraphrase.
#[tokio::test]
async fn a_spreadsheet_is_refused_and_the_refusal_names_the_agent_that_can_read_it() {
    let h = harness("agent-a25-sheet").await;
    a_file_for(
        &h.acc,
        "Budget",
        br#"{"sheets":{}}"#,
        "application/json",
        Some("sheet"),
        None,
    )
    .await;
    let (status, body) = run(&h, "file_read", json!({ "file": "Budget" })).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
    let detail = why(&body);
    assert!(detail.contains("Budget"), "{detail}");
    assert!(detail.contains("alo Sheets agent"), "{detail}");
}

// ---- extracting from an attachment ----------------------------------------------

/// **The item's second sentence.** The email is named by its subject, the
/// attachment by its filename, and what comes back is the attachment's own
/// bytes decoded — with the PDF beside it refused by name rather than described.
#[tokio::test]
async fn an_attachment_is_listed_then_read_and_a_pdf_is_refused_by_name() {
    let h = harness("agent-a25-attachment").await;
    an_email_with_attachments(&h.acc).await;

    // Nothing named: the list, so the next turn can ask which one.
    let (status, body) = run(&h, "attachment_read", json!({ "email": "Q3 figures" })).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let result = &body["result"];
    assert_eq!(result["kind"], json!("emailAttachments"));
    assert_eq!(result["email"]["subject"], json!("Q3 figures"));
    let parts = result["attachments"].as_array().unwrap();
    assert_eq!(parts.len(), 2, "{result}");
    assert_eq!(parts[0]["name"], json!("q3.csv"));
    assert_eq!(parts[0]["readable"], json!(true));
    assert_eq!(parts[1]["name"], json!("board-pack.pdf"));
    assert_eq!(
        parts[1]["readable"],
        json!(false),
        "the list says up front what cannot be opened"
    );

    // The one that is text, read.
    let (status, body) = run(
        &h,
        "attachment_read",
        json!({ "email": "Q3 figures", "attachment": "q3.csv" }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let result = &body["result"];
    assert_eq!(result["kind"], json!("emailAttachmentText"));
    assert_eq!(result["attachment"]["name"], json!("q3.csv"));
    let text = result["text"].as_str().unwrap();
    assert!(text.contains("Benelux,142000"), "{text}");
    assert!(text.contains("Nordics,98000"), "{text}");
    assert_eq!(result["truncated"], json!(false));

    // The one that is not, refused by name and by type — before a byte of it is
    // decoded.
    let (status, body) = run(
        &h,
        "attachment_read",
        json!({ "email": "Q3 figures", "attachment": "board-pack.pdf" }),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
    let detail = why(&body);
    assert!(detail.contains("board-pack.pdf"), "{detail}");
    assert!(detail.contains("application/pdf"), "{detail}");

    // An attachment that is not on the message is a refusal, not the first one.
    let (status, body) = run(
        &h,
        "attachment_read",
        json!({ "email": "Q3 figures", "attachment": "invoice.csv" }),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
    assert!(why(&body).contains("attachment"), "{body}");

    // An email nobody sent, and an email nobody named.
    let (status, body) = run(&h, "attachment_read", json!({ "email": "Q4 figures" })).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
    assert!(
        why(&body).starts_with("no email of yours matches"),
        "{body}"
    );
    let (status, body) = run(&h, "attachment_read", json!({})).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
}

/// An email with nothing attached is an answer, not a failure — the model is
/// given a reason code to say out loud instead of a refusal to paraphrase.
#[tokio::test]
async fn an_email_with_no_attachments_says_so_rather_than_failing() {
    let h = harness("agent-a25-bare").await;
    h.acc
        .deliver(b"From: paula@x.example\r\nSubject: Lunch on Thursday\r\n\r\nSee you then.\r\n")
        .await
        .unwrap();
    let (status, body) = run(
        &h,
        "attachment_read",
        json!({ "email": "Lunch on Thursday" }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["result"]["reason"], json!("noAttachments"));
    assert_eq!(body["result"]["attachments"].as_array().unwrap().len(), 0);
}

// ---- the writes ------------------------------------------------------------------

/// **The item's third sentence, end to end.** The model asked for a rename, the
/// room got a proposal and no change, and the change happened on the asker's own
/// tap — with the file's extension carried across, so it still opens.
#[tokio::test]
async fn a_rename_is_proposed_then_applied_and_the_extension_survives_it() {
    let h = harness("agent-a25-rename").await;
    let file = a_file_for(
        &h.acc,
        "scan_0012.txt",
        b"signed copy\n",
        "text/plain",
        None,
        None,
    )
    .await;
    let (base, _seen) = scripted_model(vec![wants(
        "file_rename",
        // The model drops the extension, as a model will.
        json!({ "file": "scan_0012.txt", "name": "Delaunay contract signed" }),
        "I will rename it to Delaunay contract signed.",
    )])
    .await;
    use_model(&h, &base).await;
    let agent = the_drive_agent(&h).await;
    let channel = a_room_with(&h, "the filing", &agent).await;

    let spoken = ask_in_room(&h, &channel, "@drive rename scan_0012.txt properly").await;
    let proposal = spoken["proposal"]["id"]
        .as_str()
        .expect("a write is proposed, never run")
        .to_owned();
    assert_eq!(spoken["proposal"]["tool"], json!("file_rename"));
    // Nothing has happened yet.
    assert_eq!(name_of(&h, &file).await, "scan_0012.txt");

    let decided = approve(&h, &proposal).await;
    let result = &decided["result"]["result"];
    assert_eq!(result["kind"], json!("driveFileRenamed"));
    assert_eq!(result["was"], json!("scan_0012.txt"));
    assert_eq!(result["now"], json!("Delaunay contract signed.txt"));
    assert_eq!(result["changed"], json!(true));
    assert_eq!(
        name_of(&h, &file).await,
        "Delaunay contract signed.txt",
        "an approved rename must not leave a file that no longer opens"
    );
}

/// What a rename refuses, and what it declines to call a change. Every one of
/// these is checked before the store is touched.
#[tokio::test]
async fn a_rename_is_refused_by_name_and_a_rename_to_the_same_name_writes_nothing() {
    let h = harness("agent-a25-rename-refuse").await;
    let file = a_file_for(&h.acc, "notes.txt", b"x", "text/plain", None, None).await;
    a_file_for(&h.acc, "minutes.txt", b"y", "text/plain", None, None).await;

    // A sibling already has that name: refused, naming both.
    let (status, body) = run(
        &h,
        "file_rename",
        json!({ "file": "notes.txt", "name": "minutes" }),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
    let detail = why(&body);
    assert!(detail.contains("minutes.txt"), "{detail}");
    assert!(detail.contains("notes.txt"), "{detail}");

    // A name that is a path, a blank, or nothing at all.
    for bad in [
        json!({ "file": "notes.txt", "name": "../secrets" }),
        json!({ "file": "notes.txt", "name": "sub/dir" }),
        json!({ "file": "notes.txt", "name": "   " }),
        json!({ "file": "notes.txt" }),
    ] {
        let (status, body) = run(&h, "file_rename", bad.clone()).await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{bad}: {body}");
    }
    assert_eq!(name_of(&h, &file).await, "notes.txt", "nothing was written");

    // Renaming a file to what it is already called is not a failure and not a
    // change — a reason code the model can say out loud.
    let (status, body) = run(
        &h,
        "file_rename",
        json!({ "file": "notes.txt", "name": "notes" }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["result"]["changed"], json!(false));
    assert_eq!(body["result"]["reason"], json!("nameUnchanged"));
    assert_eq!(name_of(&h, &file).await, "notes.txt");
}

/// **A move waits for a tap, and stays inside the person's own Drive.** The file
/// changes folder and nothing else: not its name, not its contents, and not who
/// can open it.
#[tokio::test]
async fn a_move_is_proposed_then_applied_into_the_folder_the_user_named() {
    let h = harness("agent-a25-move").await;
    let contracts = h
        .acc
        .drive_create_folder(&DriveLocation::Personal, None, "Contracts")
        .await
        .unwrap();
    let file = a_file_for(
        &h.acc,
        "Delaunay contract.txt",
        b"signed\n",
        "text/plain",
        None,
        None,
    )
    .await;
    let (base, _seen) = scripted_model(vec![wants(
        "file_move",
        json!({ "file": "Delaunay contract.txt", "folder": "Contracts" }),
        "I will file it under Contracts.",
    )])
    .await;
    use_model(&h, &base).await;
    let agent = the_drive_agent(&h).await;
    let channel = a_room_with(&h, "the filing", &agent).await;

    let spoken = ask_in_room(&h, &channel, "@drive file the Delaunay contract").await;
    let proposal = spoken["proposal"]["id"]
        .as_str()
        .expect("proposed")
        .to_owned();
    assert_eq!(spoken["proposal"]["tool"], json!("file_move"));
    assert_eq!(parent_of(&h, &file).await, None, "nothing has moved yet");

    let decided = approve(&h, &proposal).await;
    let result = &decided["result"]["result"];
    assert_eq!(result["kind"], json!("driveFileMoved"));
    assert_eq!(result["folder"]["name"], json!("Contracts"));
    assert_eq!(result["changed"], json!(true));
    assert_eq!(
        parent_of(&h, &file).await.as_deref(),
        Some(contracts.as_str())
    );
    assert_eq!(
        name_of(&h, &file).await,
        "Delaunay contract.txt",
        "a move renames nothing"
    );
    // Still the caller's own, personal file — a move never re-draws who can
    // read something (ADR 0027).
    let node = h.acc.drive_node(&file).await.unwrap().unwrap();
    assert_eq!(node.location_kind, "personal");
}

/// What a move refuses, and what it declines to call a change.
#[tokio::test]
async fn a_move_names_the_folders_there_are_and_refuses_a_collision() {
    let h = harness("agent-a25-move-refuse").await;
    let contracts = h
        .acc
        .drive_create_folder(&DriveLocation::Personal, None, "Contracts")
        .await
        .unwrap();
    // A nested folder, to prove the walk goes deeper than the root.
    h.acc
        .drive_create_folder(&DriveLocation::Personal, Some(&contracts), "Signed")
        .await
        .unwrap();
    let file = a_file_for(&h.acc, "deal.txt", b"x", "text/plain", None, None).await;

    // A folder that is not there: refused, and the refusal lists the ones that
    // are — recognition over recall, in a server's own words.
    let (status, body) = run(
        &h,
        "file_move",
        json!({ "file": "deal.txt", "folder": "Invoices" }),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
    let detail = why(&body);
    assert!(
        detail.contains("no folder of yours is called Invoices"),
        "{detail}"
    );
    assert!(detail.contains("Contracts"), "{detail}");
    assert!(
        detail.contains("Signed"),
        "the walk reaches a nested folder: {detail}"
    );

    // Already where it is asked to go: a reason code, not a timestamp bump.
    let (status, body) = run(&h, "file_move", json!({ "file": "deal.txt" })).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["result"]["changed"], json!(false));
    assert_eq!(body["result"]["reason"], json!("alreadyThere"));

    // Something of that name is already in the destination: refused, naming it,
    // and nothing moved. Reachable through a *folder* that shares the file's
    // name, because two **files** of one name make the file itself unnameable —
    // which the next assertion is about.
    h.acc
        .drive_create_folder(&DriveLocation::Personal, Some(&contracts), "deal.txt")
        .await
        .unwrap();
    let (status, body) = run(
        &h,
        "file_move",
        json!({ "file": "deal.txt", "folder": "Contracts" }),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
    assert!(why(&body).contains("already"), "{body}");
    assert_eq!(parent_of(&h, &file).await, None, "nothing moved");

    // And the ordinary shape of that clash: a second file of the same name
    // anywhere in the Drive makes the name ambiguous, so **every** tool refuses
    // it and says so, rather than one of them picking a file to act on. A move
    // that guessed which "deal.txt" was meant is a file nobody can find again.
    a_file_for(
        &h.acc,
        "deal.txt",
        b"y",
        "text/plain",
        None,
        Some(&contracts),
    )
    .await;
    for (tool, args) in [
        (
            "file_move",
            json!({ "file": "deal.txt", "folder": "Contracts" }),
        ),
        ("file_read", json!({ "file": "deal.txt" })),
        (
            "file_rename",
            json!({ "file": "deal.txt", "name": "signed" }),
        ),
    ] {
        let (status, body) = run(&h, tool, args).await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{tool}: {body}");
        assert!(
            why(&body).starts_with("more than one file matches deal.txt"),
            "{tool}: {body}"
        );
    }
    assert_eq!(parent_of(&h, &file).await, None, "still nothing moved");
}

// ---- isolation -------------------------------------------------------------------

/// **A file the asker could not open is not one the agent can name** — across a
/// tenant boundary and across a colleague's private Drive alike, for every tool
/// in the set, reading and writing.
///
/// The refusal is the same one an unknown name gets and never says the file
/// exists: an agent that answered differently for a real file would be a way to
/// discover somebody else's documents by asking about them.
#[tokio::test]
async fn a_file_of_another_tenant_or_another_person_cannot_be_named() {
    let h = harness("agent-a25-isolation").await;
    let other = common::harness_on(h.store.clone(), "agent-a25-stranger").await;
    a_document_for(&other.acc, "Their secret strategy").await;
    let colleague = h.ts.create_user("ben@a25-isolation.test").await.unwrap();
    let theirs = h.store.for_account(h.tenant.clone(), colleague.clone());
    let bens = a_document_for(&theirs, "Bens private appraisal").await;

    // Our own, so the refusals below are about reach and not about emptiness.
    a_document_for(&h.acc, "Handover note").await;

    for stranger in ["Their secret strategy", "Bens private appraisal"] {
        for (tool, args) in [
            ("file_read", json!({ "file": stranger })),
            ("file_rename", json!({ "file": stranger, "name": "mine" })),
            ("file_move", json!({ "file": stranger })),
        ] {
            let (status, body) = run(&h, tool, args).await;
            assert_eq!(
                status,
                StatusCode::UNPROCESSABLE_ENTITY,
                "{tool} reached {stranger}: {body}"
            );
            assert!(
                why(&body).starts_with("no file of yours is called"),
                "{tool}/{stranger}: {body}"
            );
        }
    }
    // Neither file was renamed or moved out from under its owner.
    assert_eq!(
        theirs.drive_node(&bens).await.unwrap().unwrap().name,
        "Bens private appraisal"
    );

    // And an email of another tenant's cannot be named either.
    an_email_with_attachments(&other.acc).await;
    let (status, body) = run(&h, "attachment_read", json!({ "email": "Q3 figures" })).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
    assert!(
        why(&body).starts_with("no email of yours matches"),
        "{body}"
    );
}
