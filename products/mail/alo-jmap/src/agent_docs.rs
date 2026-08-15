//! Executing the **Docs** tools of the Docs agent (ADR 0034, queue item A2.3)
//! — the acting half of what [`alo_ai::agent_docs`] describes to the model.
//!
//! The two reading tools run inside the turn ([`crate::agent_turn`]); the two
//! that change a document run only from [`crate::agent::agent_execute`], after
//! the person who asked approved the proposal. Everything here goes through the
//! caller's own tenant-scoped store handle, so the Docs agent reaches exactly
//! the documents the person who asked could already open — a document of
//! another tenant's, or of a colleague's private Drive, is not merely refused
//! here, it is not among the things that can be named
//! ([`alo_store::AccountStore::drive_docs`] and `drive_find` are personal and
//! tenant-scoped, and the resolver picks out of what they return).
//!
//! Five rules shape this module, and none of them is thin glue:
//!
//! - **Every passage comes back with its block id and its section.** A document
//!   has no cell references, so the block id the editor itself uses is the
//!   address, and the heading above it is the caption. An answer without both is
//!   one the reader has to search their own document for.
//! - **A write edits the stored tree, never a rebuild of it.**
//!   [`alo_ai::doc_blocks::set_text`] and `insert_blocks` change one part of the
//!   array as it was stored, so props, children, comments and every plugin's
//!   data survive a tool the user approved for one paragraph.
//! - **A rewrite replaces words and nothing else.** It cannot delete a block,
//!   move one, or turn a heading into a paragraph; a block whose content is a
//!   structure rather than a sentence is refused **by name, before anything is
//!   applied**, because a half-applied rewrite over somebody's prose is worse
//!   than none.
//! - **Nothing is written when nothing changed.** A rewrite whose every text is
//!   already what the block says writes no version: an approved tool must not
//!   leave a version in somebody's history saying a document changed when it did
//!   not.
//! - **Results carry facts and reason codes, never sentences.** `notRewritable`,
//!   `noSuchBlock`, `nothingToRewrite` — a user-facing sentence composed in the
//!   server would be English authored in one language, which is a bug in a
//!   European product (CLAUDE.md). The words the user reads are the model's own,
//!   in their language.

use std::time::{SystemTime, UNIX_EPOCH};

use axum::Json;
use axum::http::StatusCode;
use bytes::Bytes;
use serde_json::{Value, json};

use alo_ai::doc_blocks::{
    self, DocBlock, DocError, Document, NEW_BLOCK_KINDS, WriteError, new_block,
};
use alo_ai::sheet_grid::search_terms;
use alo_store::{BlobId, DriveNode, DriveNodeId};

use crate::agent_args::{pick, string_arg, unprocessable};
use crate::billing::map_store_err;
use crate::error::Problem;
use crate::state::Account;

/// The largest document this reads. A block tree is JSON, so a long report is a
/// few hundred kilobytes; well past this the parse costs more than the turn is
/// worth, and the honest answer is to say so rather than to spend a minute on
/// it.
const MAX_DOCUMENT_BYTES: i64 = 8 * 1024 * 1024;

/// How many blocks one `doc_read` hands back at most, and its default.
const MAX_READ_BLOCKS: usize = 60;
const DEFAULT_READ_BLOCKS: usize = 30;

/// How many passages one `doc_answer` matches, and how many blocks of the
/// section under a matched heading come with it.
const MAX_ANSWER_BLOCKS: usize = 8;
const MAX_SECTION_BLOCKS: usize = 6;

/// How much of one block's text is shown. A block longer than this is reported
/// truncated rather than silently cut.
const MAX_BLOCK_CHARS: usize = 1_200;

/// How many blocks one approved draft may add, and one approved rewrite may
/// change.
const MAX_DRAFT_BLOCKS: usize = 40;
const MAX_REWRITE_BLOCKS: usize = 60;

/// The longest text one written block may carry.
const MAX_TEXT_CHARS: usize = 4_000;

// ---- the reading tools -------------------------------------------------------

/// `doc_read` — a window of a document, block by block, with ids.
///
/// The window is bounded and says when it was cut, because the alternative — a
/// document handed over entire — is both a turn that does not fit and an answer
/// the model would believe was complete.
///
/// # Errors
/// `422` when no document of the caller's matches, when the starting block is
/// not in it, or when the stored blob is not a document; the store's own
/// failure otherwise.
pub async fn execute_doc_read(account: &Account, args: &Value) -> Result<Json<Value>, Problem> {
    let (node, _, document) = load(account, args).await?;
    let start = match string_arg(args, "from") {
        Some(from) => {
            document
                .block(&from)
                .ok_or_else(|| {
                    unprocessable(format!("no block of this document is called {from}"))
                })?
                .position
        }
        None => 1,
    };
    let wanted = args
        .get("blocks")
        .and_then(Value::as_u64)
        .and_then(|blocks| usize::try_from(blocks).ok())
        .unwrap_or(DEFAULT_READ_BLOCKS)
        .clamp(1, MAX_READ_BLOCKS);

    let shown: Vec<&DocBlock> = document
        .blocks
        .iter()
        .filter(|block| block.position >= start)
        .take(wanted)
        .collect();
    let truncated = document.blocks.len() > start.saturating_sub(1) + shown.len();

    Ok(Json(json!({
        "ok": true,
        "result": {
            "kind": "docRead",
            "document": document_ref(&node, &document),
            "from": start,
            "blocks": shown.iter().map(|block| block_json(&document, block)).collect::<Vec<_>>(),
            // Said plainly: what came back is a window, and there is more.
            "truncated": truncated,
        }
    })))
}

/// `doc_answer` — the passages of a document that mention what was asked about.
///
/// Every returned passage carries its block id and the heading it sits under,
/// so the model answers from the document rather than from the one word that
/// matched.
///
/// **A matched heading brings the section under it.** This is the document's
/// version of a spreadsheet's whole row: somebody asking "what do we say about
/// payment terms" matches the *heading* called "Payment terms", while the
/// sentence that answers them is the paragraph below it, holding none of the
/// words they typed. Each block says whether it matched or came along as the
/// body of one that did, so nothing here pretends the search found more than it
/// did.
///
/// # Errors
/// `422` when no document matches, when nothing was asked, or when the stored
/// blob is not a document; the store's own failure otherwise.
pub async fn execute_doc_answer(account: &Account, args: &Value) -> Result<Json<Value>, Problem> {
    let question = string_arg(args, "question")
        .ok_or_else(|| unprocessable("what to look for in the document is required"))?;
    let (node, _, document) = load(account, args).await?;
    let terms = search_terms(&question);
    let found = document.find(&terms, MAX_ANSWER_BLOCKS);
    let matched: Vec<usize> = found.iter().map(|block| block.position).collect();

    // The matches, each followed by its section, in the order they read on the
    // page — a passage out of order is one the reader cannot follow.
    let mut shown: Vec<&DocBlock> = Vec::new();
    for block in &found {
        for part in std::iter::once(*block)
            .chain(document.section_under(block, MAX_SECTION_BLOCKS))
            .filter(|part| !part.text.trim().is_empty())
        {
            if !shown.iter().any(|seen| seen.position == part.position) {
                shown.push(part);
            }
        }
    }
    shown.sort_by_key(|block| block.position);
    shown.truncate(MAX_ANSWER_BLOCKS + MAX_SECTION_BLOCKS);

    Ok(Json(json!({
        "ok": true,
        "result": {
            "kind": "docAnswer",
            "document": document_ref(&node, &document),
            "terms": terms,
            "searchedBlocks": document.blocks.len(),
            "matched": matched.len(),
            "blocks": shown.iter().map(|block| {
                let mut value = block_json(&document, block);
                if let Some(object) = value.as_object_mut() {
                    object.insert(
                        "matched".to_owned(),
                        json!(matched.contains(&block.position)),
                    );
                }
                value
            }).collect::<Vec<_>>(),
        }
    })))
}

// ---- the writing tools ---------------------------------------------------------

/// One block a draft was asked to add.
struct Draft {
    kind: String,
    level: Option<u64>,
    text: String,
}

/// `doc_draft_section` — new blocks into a document, after one of its own.
///
/// It only adds. Nothing here can delete a block, replace one or move one, so
/// an approved draft cannot lose a sentence somebody wrote — the worst it can
/// do is put a paragraph where it was not wanted, which the version history
/// undoes.
///
/// # Errors
/// `422` for a missing or malformed block list, a kind the editor does not
/// have, an `after` naming no block, or text past the limit; the store's own
/// failure otherwise.
pub async fn execute_doc_draft_section(
    account: &Account,
    args: &Value,
) -> Result<Json<Value>, Problem> {
    let (node, mut raw, document) = load(account, args).await?;
    let drafts = draft_list(args)?;
    // Resolved before anything is written, so an `after` that no longer exists
    // refuses the whole draft rather than appending it somewhere else.
    let after = match string_arg(args, "after") {
        Some(wanted) => Some(
            document
                .block(&wanted)
                .ok_or_else(|| {
                    unprocessable(format!("no block of this document is called {wanted}"))
                })?
                .clone(),
        ),
        None => None,
    };

    let ids = fresh_ids(&document, drafts.len());
    let blocks: Vec<Value> = drafts
        .iter()
        .zip(ids.iter())
        .map(|(draft, id)| new_block(id, &draft.kind, draft.level, &draft.text))
        .collect();
    doc_blocks::insert_blocks(
        &mut raw,
        after.as_ref().map(|block| block.path.as_slice()),
        blocks,
    )
    .map_err(write_problem)?;

    let version = save(account, &node, &raw).await?;
    Ok(Json(json!({
        "ok": true,
        "result": {
            "kind": "docDraftSection",
            "document": document_ref(&node, &document),
            "after": after.as_ref().map(block_ref),
            "added": drafts.iter().zip(ids.iter()).map(|(draft, id)| json!({
                "block": id,
                "kind": draft.kind,
                "level": draft.level,
                "text": draft.text,
            })).collect::<Vec<_>>(),
            "versionNo": version,
        }
    })))
}

/// One block a rewrite was asked to change.
struct Rewrite {
    block: DocBlock,
    text: String,
}

/// `doc_rewrite` — new text into blocks that already exist.
///
/// This is both "rewrite this selection" and "translate this document": the
/// words are the model's, in the user's language, and what happens to the
/// document is the same either way. Two refusals do the work. A block that is
/// not in the document any more, and a block whose content is a structure
/// rather than a sentence, are both refused **by name and before anything is
/// applied** — a rewrite that fails on its fortieth block having applied
/// thirty-nine is a document nobody can reason about.
///
/// # Errors
/// `422` for a missing or malformed block list, an id naming no block, a
/// duplicate id, a block whose text cannot be replaced, or text past the limit;
/// the store's own failure otherwise.
pub async fn execute_doc_rewrite(account: &Account, args: &Value) -> Result<Json<Value>, Problem> {
    let (node, mut raw, document) = load(account, args).await?;
    let rewrites = rewrite_list(args, &document)?;

    // A rewrite that changes nothing writes no version. The blocks are still
    // reported, so the model can say plainly that the wording was already what
    // was asked for rather than claiming to have changed it.
    let changed: Vec<&Rewrite> = rewrites
        .iter()
        .filter(|rewrite| rewrite.block.text != rewrite.text)
        .collect();
    for rewrite in &changed {
        doc_blocks::set_text(&mut raw, &rewrite.block.path, &rewrite.text)
            .map_err(write_problem)?;
    }
    let version = if changed.is_empty() {
        None
    } else {
        Some(save(account, &node, &raw).await?)
    };

    Ok(Json(json!({
        "ok": true,
        "result": {
            "kind": "docRewrite",
            "document": document_ref(&node, &document),
            "changed": changed.len(),
            "blocks": rewrites.iter().map(|rewrite| json!({
                "block": rewrite.block.id,
                "kind": rewrite.block.kind,
                "was": rewrite.block.text,
                "now": rewrite.text,
                "changed": rewrite.block.text != rewrite.text,
            })).collect::<Vec<_>>(),
            "versionNo": version,
            "reason": if changed.is_empty() { json!("nothingToRewrite") } else { Value::Null },
        }
    })))
}

// ---- resolving, loading, saving --------------------------------------------------

/// The document an argument names, its stored blob, and that blob read.
///
/// Names, never ids: the candidates come from the caller's own Drive, so a
/// document belonging to another tenant — or to a colleague — is not among the
/// things that can be named here.
async fn load(account: &Account, args: &Value) -> Result<(DriveNode, Value, Document), Problem> {
    let node = resolve_document(account, args).await?;
    let raw = read_blocks(account, &node).await?;
    let document = Document::read(&raw).map_err(doc_problem)?;
    Ok((node, raw, document))
}

/// The document node an argument names, or the caller's only one.
async fn resolve_document(account: &Account, args: &Value) -> Result<DriveNode, Problem> {
    let Some(wanted) = string_arg(args, "document") else {
        let mut docs = account.acc.drive_docs(50).await.map_err(map_store_err)?;
        return match docs.len() {
            0 => Err(unprocessable("there is no document in your drive yet")),
            1 => Ok(docs.remove(0)),
            _ => Err(unprocessable(format!(
                "more than one document: {} — say which",
                names(&docs)
            ))),
        };
    };
    // Searched by name first, so a drive with more documents than one listing
    // holds is still reachable; the listing is what a refusal names.
    let found: Vec<DriveNode> = account
        .acc
        .drive_find(&wanted, 20)
        .await
        .map_err(map_store_err)?
        .into_iter()
        .filter(|node| node.kind == "doc")
        .collect();
    if found.is_empty() {
        let docs = account.acc.drive_docs(50).await.map_err(map_store_err)?;
        return Err(unprocessable(if docs.is_empty() {
            "there is no document in your drive yet".to_owned()
        } else {
            format!(
                "no document of yours is called {wanted} — you have: {}",
                names(&docs)
            )
        }));
    }
    pick(
        &wanted,
        found
            .iter()
            .map(|node| (node.name.as_str(), node.clone()))
            .collect(),
        "document",
    )
}

/// The names of some documents, for a refusal that lists them.
fn names(nodes: &[DriveNode]) -> String {
    nodes
        .iter()
        .map(|node| node.name.as_str())
        .collect::<Vec<_>>()
        .join(", ")
}

/// The stored block array of a document node.
async fn read_blocks(account: &Account, node: &DriveNode) -> Result<Value, Problem> {
    let Some(blob) = node.blob_id.clone() else {
        // A doc node with no blob has never been saved: it is a new, empty
        // document, and that is an answer rather than a failure.
        return Ok(json!([]));
    };
    if node.size > MAX_DOCUMENT_BYTES {
        return Err(unprocessable(
            "that document is too large for the agent to read",
        ));
    }
    let bytes = account
        .acc
        .blob_bytes_for_send(&BlobId::new(blob))
        .await
        .map_err(map_store_err)?;
    if bytes.is_empty() {
        return Ok(json!([]));
    }
    serde_json::from_slice(&bytes).map_err(|_| doc_problem(DocError::NotJson))
}

/// Stores an edited document as a new version of the node, exactly as the
/// editor's own save does (a blob, then a version) — so the agent's change is in
/// the same history as everybody else's and can be rolled back the same way.
async fn save(account: &Account, node: &DriveNode, raw: &Value) -> Result<i32, Problem> {
    let bytes = serde_json::to_vec(raw).map_err(|_| Problem::server_error())?;
    let size = i64::try_from(bytes.len()).map_err(|_| Problem::server_error())?;
    let blob = account
        .acc
        .put_blob(Bytes::from(bytes), Some("application/json"))
        .await
        .map_err(map_store_err)?;
    account
        .acc
        .drive_add_version(
            &DriveNodeId::new(node.id.as_str().to_owned()),
            blob.as_str(),
            size,
        )
        .await
        .map_err(map_store_err)
}

/// Ids for blocks about to be added, none of which the document already holds.
///
/// The clock and the index make them unique; [`doc_blocks::holds_id`] proves it
/// against the document in hand, because BlockNote reads a repeated id as one
/// block in two places and quietly loses one of them.
fn fresh_ids(document: &Document, count: usize) -> Vec<String> {
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |since| since.as_nanos());
    let mut out = Vec::with_capacity(count);
    let mut nudge = 0_u32;
    for index in 0..count {
        let mut id = format!("alo-{stamp}-{index}");
        while doc_blocks::holds_id(document, &id) || out.contains(&id) {
            nudge += 1;
            id = format!("alo-{stamp}-{index}-{nudge}");
        }
        out.push(id);
    }
    out
}

/// The `422` a blob that cannot be read as a document earns, carrying the
/// reason code rather than a sentence.
fn doc_problem(error: DocError) -> Problem {
    Problem::with(
        StatusCode::UNPROCESSABLE_ENTITY,
        format!("that document cannot be read: {}", error.as_str()),
    )
}

/// The `422` a write that could not be applied earns. Both of its reasons are
/// caught before anything is written; this is the backstop that keeps a
/// half-applied document impossible rather than unlikely.
fn write_problem(error: WriteError) -> Problem {
    Problem::with(
        StatusCode::UNPROCESSABLE_ENTITY,
        format!("that block cannot be written: {}", error.as_str()),
    )
}

/// The `blocks` argument of a draft, validated whole.
fn draft_list(args: &Value) -> Result<Vec<Draft>, Problem> {
    let entries = block_entries(args, MAX_DRAFT_BLOCKS, "say what to add to the document")?;
    let mut drafts: Vec<Draft> = Vec::with_capacity(entries.len());
    for (index, entry) in entries.iter().enumerate() {
        let at = index + 1;
        let kind = string_arg(entry, "kind").unwrap_or_else(|| "paragraph".to_owned());
        if !NEW_BLOCK_KINDS.contains(&kind.as_str()) {
            return Err(unprocessable(format!(
                "block {at} asks for a {kind}, and a document has: {}",
                NEW_BLOCK_KINDS.join(", ")
            )));
        }
        let text = text_arg(entry, at)?;
        drafts.push(Draft {
            level: entry.get("level").and_then(Value::as_u64),
            kind,
            text,
        });
    }
    Ok(drafts)
}

/// The `blocks` argument of a rewrite, validated whole against the document it
/// is about.
fn rewrite_list(args: &Value, document: &Document) -> Result<Vec<Rewrite>, Problem> {
    let entries = block_entries(
        args,
        MAX_REWRITE_BLOCKS,
        "say which blocks to rewrite, and what they should say",
    )?;
    let mut rewrites: Vec<Rewrite> = Vec::with_capacity(entries.len());
    for (index, entry) in entries.iter().enumerate() {
        let at = index + 1;
        let wanted = string_arg(entry, "block")
            .ok_or_else(|| unprocessable(format!("entry {at} does not say which block")))?;
        let block = document.block(&wanted).ok_or_else(|| {
            unprocessable(format!("no block of this document is called {wanted}"))
        })?;
        // A table, an image, a file: their text is a structure, and one
        // sentence where the structure was would destroy it.
        if !block.rewritable {
            return Err(unprocessable(format!(
                "{wanted} is a {} and its text cannot be replaced",
                if block.kind.is_empty() {
                    "block of an unknown kind"
                } else {
                    block.kind.as_str()
                }
            )));
        }
        if rewrites.iter().any(|other| other.block.path == block.path) {
            return Err(unprocessable(format!(
                "{wanted} is named twice, so what it should say is not stated"
            )));
        }
        rewrites.push(Rewrite {
            block: block.clone(),
            text: text_arg(entry, at)?,
        });
    }
    Ok(rewrites)
}

/// The `blocks` array both writes take, checked for being there and for size.
fn block_entries<'a>(
    args: &'a Value,
    most: usize,
    missing: &str,
) -> Result<&'a Vec<Value>, Problem> {
    let entries = args
        .get("blocks")
        .and_then(Value::as_array)
        .ok_or_else(|| unprocessable(missing.to_owned()))?;
    if entries.is_empty() {
        return Err(unprocessable(missing.to_owned()));
    }
    if entries.len() > most {
        return Err(unprocessable(format!(
            "at most {most} blocks at a time, and {} were asked for",
            entries.len()
        )));
    }
    Ok(entries)
}

/// One entry's `text`, present and within the limit.
fn text_arg(entry: &Value, at: usize) -> Result<String, Problem> {
    let text = string_arg(entry, "text")
        .ok_or_else(|| unprocessable(format!("block {at} has no text")))?;
    if text.chars().count() > MAX_TEXT_CHARS {
        return Err(unprocessable(format!(
            "block {at} is longer than {MAX_TEXT_CHARS} characters"
        )));
    }
    Ok(text)
}

// ---- the shapes a result carries -------------------------------------------------

/// Which document a result is about.
fn document_ref(node: &DriveNode, document: &Document) -> Value {
    json!({
        "id": node.id.as_str(),
        "name": node.name,
        "blocks": document.blocks.len(),
        "words": document.words(),
    })
}

/// One block, by the address a write is given.
fn block_ref(block: &DocBlock) -> Value {
    json!({
        "block": block.id,
        "position": block.position,
        "kind": block.kind,
    })
}

/// One block: its id, its kind, where it sits, and what it says.
fn block_json(document: &Document, block: &DocBlock) -> Value {
    let cut = block.text.chars().count() > MAX_BLOCK_CHARS;
    json!({
        "block": block.id,
        "position": block.position,
        "kind": block.kind,
        "level": block.level,
        "depth": block.depth,
        // The document's own caption for this passage — what makes an answer
        // findable in the file rather than only quotable.
        "section": document.heading_above(block).map(|heading| heading.text.clone()),
        "text": block.text.chars().take(MAX_BLOCK_CHARS).collect::<String>(),
        "truncated": cut,
        // Said in the read, so a rewrite is never proposed for a block that
        // would refuse it.
        "rewritable": block.rewritable,
    })
}
