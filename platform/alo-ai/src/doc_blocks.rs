//! Reading and writing a document's stored block tree (queue item A2.3).
//!
//! An alo Doc is a Drive node of kind `doc` whose blob is the editor's own
//! BlockNote block array (`web/src/drive/DocEditor.tsx`, ADR 0031). Everything
//! the Docs agent does — answering from the text, drafting a section, rewriting
//! a passage, translating one — is a read or a write of that array, and all of
//! it is here: pure, synchronous, no store handle, no model. The executors in
//! `alo-jmap`'s `agent_docs` fetch the bytes and put them back.
//!
//! Four rules shape this module, and each is a mistake it exists to prevent:
//!
//! - **A passage is cited, never summarised.** Everything read out carries the
//!   id of the block it came from and the heading it sits under, because the
//!   item's requirement is an agent whose sentences a person can check against
//!   their own document. [`DocBlock::id`] is what a write is addressed to, and
//!   it is the editor's own id — not a position, which moves the moment
//!   somebody adds a paragraph above it.
//! - **A write edits the document it was given.** [`set_text`] and
//!   [`insert_blocks`] reach into the caller's own parsed tree and change one
//!   part of it. Nothing here re-serialises a document from the model below: a
//!   block carries props, children, comments and plugin data this file does not
//!   model, and rebuilding it from what we understood would silently delete all
//!   of them.
//! - **Nothing here writes prose.** The words a rewrite puts in a block are the
//!   caller's, which is to say the model's, in the user's language. This file
//!   moves text; it does not compose, translate or summarise any.
//! - **Everything is bounded.** A document can hold thousands of blocks; a turn
//!   holds a few dozen. Every reader takes a limit and reports having hit it, so
//!   a truncated read is never mistaken for a whole document.

use serde_json::{Map, Value};

/// The largest depth of nesting walked. A block tree is a list with children;
/// well past this it is a pathological or hostile document, and the honest
/// answer is to stop walking rather than to recurse on it.
const MAX_DEPTH: u32 = 8;

/// Why a stored blob could not be read as a document.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DocError {
    /// The blob was not JSON at all.
    NotJson,
    /// JSON, but not a document: the editor stores an array of blocks.
    NotADocument,
}

impl DocError {
    /// The reason code the tool result carries. A code and not a sentence: the
    /// words a user reads are the model's or the client's, in their language.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::NotJson => "notJson",
            Self::NotADocument => "notADocument",
        }
    }
}

/// The block types this module will create, and the word each is addressed by.
///
/// Deliberately the plain ones. A table, an image, a code block and a file are
/// all structures with their own inner shape, and composing one from a sentence
/// of intent is the mistake A2.2b was cut for — an approved proposal that puts
/// an object in somebody's document which their editor cannot open.
pub const NEW_BLOCK_KINDS: &[&str] = &[
    "paragraph",
    "heading",
    "bulletListItem",
    "numberedListItem",
    "checkListItem",
    "quote",
];

/// One block of a document, as the stored tree holds it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DocBlock {
    /// The editor's own id for this block — **the address a write is given**,
    /// because it survives an edit above it and a position does not. `None` for
    /// a document written by something that did not set one, which the read
    /// reports rather than inventing.
    pub id: Option<String>,
    /// The block's type: `paragraph`, `heading`, `bulletListItem`, `table`, …
    pub kind: String,
    /// A heading's level (1, 2, 3), from its props.
    pub level: Option<u64>,
    /// The block's text, inline styling flattened away.
    pub text: String,
    /// How deep in the tree it sits — 0 for a top-level block.
    pub depth: u32,
    /// Its position in the flattened document, 1-based, for a person reading a
    /// transcript. Never used to address a write.
    pub position: usize,
    /// The path of child indexes from the root to this block.
    pub path: Vec<usize>,
    /// Whether [`set_text`] can replace this block's text.
    ///
    /// A paragraph can be rewritten; a table cannot, because its text is a grid
    /// and putting one sentence where the grid was would destroy it. Reported
    /// so a refusal names the block rather than the tool failing at the end.
    pub rewritable: bool,
}

impl DocBlock {
    /// Whether this block's text contains a term (case-insensitively).
    #[must_use]
    pub fn contains(&self, term: &str) -> bool {
        self.text.to_lowercase().contains(term)
    }

    /// Whether this block is a heading.
    #[must_use]
    pub fn is_heading(&self) -> bool {
        self.kind == "heading"
    }
}

/// A document's block tree, read.
#[derive(Debug, Clone)]
pub struct Document {
    /// Every block, depth first, in the order they read on the page.
    pub blocks: Vec<DocBlock>,
}

impl Document {
    /// Reads a stored block array.
    ///
    /// Tolerant on purpose about everything that is not structure: a block with
    /// no id, no type or no content is reported as what it is rather than
    /// failing the read, because a document that has been through an import, an
    /// AI insert and three versions of an editor is not a document we get to be
    /// strict with. The one thing refused is a blob that is not an array of
    /// blocks at all, which leaves nothing to read.
    ///
    /// # Errors
    /// [`DocError::NotADocument`] when the blob is not a JSON array.
    pub fn read(raw: &Value) -> Result<Self, DocError> {
        let top = raw.as_array().ok_or(DocError::NotADocument)?;
        let mut blocks = Vec::new();
        walk(top, 0, &mut Vec::new(), &mut blocks);
        Ok(Self { blocks })
    }

    /// The block an argument names — by the editor's own id, or by its
    /// position (`3`, `#3`) for a block that has no id.
    ///
    /// An id wins over a position, so a document whose block is literally
    /// called "3" is still addressable.
    #[must_use]
    pub fn block(&self, wanted: &str) -> Option<&DocBlock> {
        let wanted = wanted.trim();
        if wanted.is_empty() {
            return None;
        }
        if let Some(found) = self
            .blocks
            .iter()
            .find(|block| block.id.as_deref() == Some(wanted))
        {
            return Some(found);
        }
        let position: usize = wanted.trim_start_matches('#').parse().ok()?;
        self.blocks.iter().find(|block| block.position == position)
    }

    /// The heading a block sits under — the nearest heading above it.
    ///
    /// The document's equivalent of a spreadsheet's column label: it is what
    /// turns "the document says 30 days" into "under *Payment terms*, the
    /// document says 30 days", which is the difference between an answer
    /// somebody can find again and one they have to take on trust.
    #[must_use]
    pub fn heading_above(&self, block: &DocBlock) -> Option<&DocBlock> {
        self.blocks
            .iter()
            .take(block.position.saturating_sub(1))
            .rfind(|candidate| candidate.is_heading() && !candidate.text.trim().is_empty())
    }

    /// The blocks under a heading — everything down to the next heading of the
    /// same level or higher, up to `limit`.
    ///
    /// A document's answer to "the whole row". A question about payment terms
    /// matches the *heading* called "Payment terms", and the sentence that
    /// answers it is the paragraph underneath, which holds none of the words
    /// asked about. Returning the heading alone would be an agent saying "your
    /// document has a section about that" to somebody who asked what it says.
    #[must_use]
    pub fn section_under(&self, heading: &DocBlock, limit: usize) -> Vec<&DocBlock> {
        if !heading.is_heading() {
            return Vec::new();
        }
        let level = heading.level.unwrap_or(1);
        self.blocks
            .iter()
            .skip(heading.position)
            .take_while(|block| !(block.is_heading() && block.level.unwrap_or(1) <= level))
            .take(limit)
            .collect()
    }

    /// Every block holding any of the terms, best first.
    ///
    /// "Best" is how many distinct terms the block holds, then how far up the
    /// document it is. A heading is searched like any other block: a question
    /// about "payment terms" is often answered by the section called that, and
    /// [`Self::section_under`] is what turns that match into the answer.
    #[must_use]
    pub fn find(&self, terms: &[String], limit: usize) -> Vec<&DocBlock> {
        if terms.is_empty() {
            return Vec::new();
        }
        let mut scored: Vec<(usize, usize, &DocBlock)> = Vec::new();
        for block in &self.blocks {
            if block.text.trim().is_empty() {
                continue;
            }
            let distinct = terms.iter().filter(|term| block.contains(term)).count();
            if distinct > 0 {
                scored.push((distinct, block.position, block));
            }
        }
        scored.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
        scored
            .into_iter()
            .take(limit)
            .map(|(_, _, block)| block)
            .collect()
    }

    /// How many words the whole document holds — the count the editor shows in
    /// its own footer, worked out the same way.
    #[must_use]
    pub fn words(&self) -> usize {
        self.blocks
            .iter()
            .map(|block| block.text.split_whitespace().count())
            .sum()
    }
}

/// Walks the tree depth first, flattening it into blocks that remember where
/// they came from.
fn walk(nodes: &[Value], depth: u32, path: &mut Vec<usize>, out: &mut Vec<DocBlock>) {
    for (index, node) in nodes.iter().enumerate() {
        let Some(block) = node.as_object() else {
            continue;
        };
        path.push(index);
        let kind = block
            .get("type")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned();
        let content = block.get("content");
        out.push(DocBlock {
            id: block
                .get("id")
                .and_then(Value::as_str)
                .filter(|id| !id.is_empty())
                .map(str::to_owned),
            level: block
                .get("props")
                .and_then(|props| props.get("level"))
                .and_then(Value::as_u64),
            text: inline_text(content),
            depth,
            position: out.len() + 1,
            path: path.clone(),
            // Only a block whose content is a list of inline pieces can have
            // that list replaced. A table's content is a grid object and an
            // image's is nothing at all.
            rewritable: content.is_some_and(Value::is_array) && !kind.is_empty(),
            kind,
        });
        if depth < MAX_DEPTH
            && let Some(children) = block.get("children").and_then(Value::as_array)
        {
            walk(children, depth + 1, path, out);
        }
        path.pop();
    }
}

/// The text of a block's content, inline styling flattened away.
///
/// Walks `content` and `rows`/`cells` alike, so a table's text is readable even
/// though its structure is not rewritable — an agent that could not read the
/// table would answer "the document does not say" about something it does.
///
/// **The runs of one sentence are joined exactly as they were written.** A bold
/// phrase in the middle of a paragraph is three runs, and the spaces around it
/// belong to the runs beside it; putting a separator between them gives back a
/// sentence with a double space in it, which is a sentence the user cannot find
/// in their own document by searching for it. Only a cell of a table is
/// separated from the next, because there the boundary is real.
fn inline_text(value: Option<&Value>) -> String {
    let mut out = String::new();
    collect_text(value, &mut out, 0);
    out.trim().to_owned()
}

fn collect_text(value: Option<&Value>, out: &mut String, depth: u32) {
    if depth > MAX_DEPTH {
        return;
    }
    match value {
        Some(Value::Array(items)) => {
            for item in items {
                collect_text(Some(item), out, depth + 1);
            }
        }
        Some(Value::Object(node)) => {
            if let Some(text) = node.get("text").and_then(Value::as_str) {
                out.push_str(text);
            }
            // A link's words are inside it.
            collect_text(node.get("content"), out, depth + 1);
            // A table's are inside its rows, and one cell does not run into the
            // next.
            for key in ["rows", "cells"] {
                if let Some(list) = node.get(key).and_then(Value::as_array) {
                    for item in list {
                        separate(out);
                        collect_text(Some(item), out, depth + 1);
                    }
                }
            }
        }
        _ => {}
    }
}

/// A single space between two things that were never one sentence.
fn separate(out: &mut String) {
    if !out.is_empty() && !out.ends_with(' ') {
        out.push(' ');
    }
}

// ---- writing -----------------------------------------------------------------

/// Why a write could not be applied.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WriteError {
    /// The path named no block — a document edited between the read and the
    /// approval.
    NoSuchBlock,
    /// The block's content is not a list of inline pieces (a table, an image),
    /// so replacing its text would destroy its structure.
    NotRewritable,
}

impl WriteError {
    /// The reason code the tool result carries.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::NoSuchBlock => "noSuchBlock",
            Self::NotRewritable => "notRewritable",
        }
    }
}

/// Replaces one block's text, keeping everything else about that block.
///
/// The type, the level, the alignment, the colours, the children and any key a
/// plugin left there all survive: a rewrite from a chat message is not a reason
/// for somebody's heading to become a paragraph. **The styling of the first
/// inline run is carried onto the new text** — a paragraph that was entirely
/// bold stays bold — because the alternative, dropping to unstyled text, is a
/// change to the document nobody asked for. Styling that varied *within* the
/// sentence goes with the sentence it belonged to, which is what replacing a
/// sentence means.
///
/// # Errors
/// [`WriteError`] when the path names no block, or names one whose text cannot
/// be replaced.
pub fn set_text(raw: &mut Value, path: &[usize], text: &str) -> Result<(), WriteError> {
    let block = block_at(raw, path).ok_or(WriteError::NoSuchBlock)?;
    let styles = block
        .get("content")
        .and_then(Value::as_array)
        .ok_or(WriteError::NotRewritable)?
        .iter()
        .find_map(|run| run.get("styles").cloned())
        .unwrap_or_else(|| Value::Object(Map::new()));
    let mut run = Map::new();
    run.insert("type".to_owned(), Value::String("text".to_owned()));
    run.insert("text".to_owned(), Value::String(text.to_owned()));
    run.insert("styles".to_owned(), styles);
    block.insert("content".to_owned(), Value::Array(vec![Value::Object(run)]));
    Ok(())
}

/// Puts new blocks into the document after the block at `path`, or at the end
/// when no path is given.
///
/// Inserted **beside** the block named, at its own level, so a section drafted
/// after a paragraph inside a list item does not become part of that list item.
/// Nothing else in the document moves.
///
/// # Errors
/// [`WriteError::NoSuchBlock`] when the path names no block.
pub fn insert_blocks(
    raw: &mut Value,
    after: Option<&[usize]>,
    blocks: Vec<Value>,
) -> Result<usize, WriteError> {
    let Some(path) = after else {
        let top = raw.as_array_mut().ok_or(WriteError::NoSuchBlock)?;
        let at = top.len();
        top.extend(blocks);
        return Ok(at);
    };
    let (parent_path, index) = path.split_at(path.len().saturating_sub(1));
    let index = *index.first().ok_or(WriteError::NoSuchBlock)?;
    let siblings = siblings_at(raw, parent_path).ok_or(WriteError::NoSuchBlock)?;
    if index >= siblings.len() {
        return Err(WriteError::NoSuchBlock);
    }
    let at = index + 1;
    for (offset, block) in blocks.into_iter().enumerate() {
        siblings.insert(at + offset, block);
    }
    Ok(at)
}

/// A new block of one of [`NEW_BLOCK_KINDS`], with the id the caller minted.
///
/// The props are the editor's own defaults, and a heading carries its level.
/// Nothing else is set: a block that named a colour or an alignment would be
/// the agent deciding how somebody's document looks.
#[must_use]
pub fn new_block(id: &str, kind: &str, level: Option<u64>, text: &str) -> Value {
    let mut props = Map::new();
    props.insert("textColor".to_owned(), Value::String("default".to_owned()));
    props.insert(
        "backgroundColor".to_owned(),
        Value::String("default".to_owned()),
    );
    props.insert("textAlignment".to_owned(), Value::String("left".to_owned()));
    if kind == "heading" {
        props.insert(
            "level".to_owned(),
            Value::from(level.unwrap_or(2).clamp(1, 3)),
        );
    }
    let mut run = Map::new();
    run.insert("type".to_owned(), Value::String("text".to_owned()));
    run.insert("text".to_owned(), Value::String(text.to_owned()));
    run.insert("styles".to_owned(), Value::Object(Map::new()));
    let mut block = Map::new();
    block.insert("id".to_owned(), Value::String(id.to_owned()));
    block.insert("type".to_owned(), Value::String(kind.to_owned()));
    block.insert("props".to_owned(), Value::Object(props));
    block.insert(
        "content".to_owned(),
        Value::Array(if text.is_empty() {
            Vec::new()
        } else {
            vec![Value::Object(run)]
        }),
    );
    block.insert("children".to_owned(), Value::Array(Vec::new()));
    Value::Object(block)
}

/// Whether an id is already somewhere in the document — asked before a new
/// block is inserted, because BlockNote reads a repeated id as one block in two
/// places and the second one wins.
#[must_use]
pub fn holds_id(document: &Document, id: &str) -> bool {
    document
        .blocks
        .iter()
        .any(|block| block.id.as_deref() == Some(id))
}

/// The block object at a path, for writing.
fn block_at<'a>(raw: &'a mut Value, path: &[usize]) -> Option<&'a mut Map<String, Value>> {
    let (parent_path, index) = path.split_at(path.len().checked_sub(1)?);
    let index = *index.first()?;
    siblings_at(raw, parent_path)?
        .get_mut(index)?
        .as_object_mut()
}

/// The list a block at `path` lives in — the document itself for a top-level
/// block, or some block's `children` for a nested one.
fn siblings_at<'a>(raw: &'a mut Value, path: &[usize]) -> Option<&'a mut Vec<Value>> {
    let mut list = raw.as_array_mut()?;
    for step in path {
        list = list
            .get_mut(*step)?
            .as_object_mut()?
            .get_mut("children")?
            .as_array_mut()?;
    }
    Some(list)
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use serde_json::json;

    /// A document shaped the way a real one is: a heading, prose under it, a
    /// list with a nested paragraph, a partly-bold paragraph, a table, and a
    /// block with no id at all — every one of them something the agent must
    /// handle rather than trip on.
    fn document() -> Value {
        json!([
            {"id": "b1", "type": "heading", "props": {"level": 1}, "content": [
                {"type": "text", "text": "Payment terms", "styles": {}}
            ], "children": []},
            {"id": "b2", "type": "paragraph", "props": {}, "content": [
                {"type": "text", "text": "Invoices are due within ", "styles": {}},
                {"type": "text", "text": "30 days", "styles": {"bold": true}}
            ], "children": []},
            {"id": "b3", "type": "bulletListItem", "props": {}, "content": [
                {"type": "text", "text": "Late payment is charged monthly.", "styles": {}}
            ], "children": [
                {"id": "b4", "type": "paragraph", "props": {}, "content": [
                    {"type": "text", "text": "See the annex for the rate.", "styles": {}}
                ], "children": []}
            ]},
            {"id": "b5", "type": "table", "props": {}, "content":
                {"type": "tableContent", "rows": [
                    {"cells": [[{"type": "text", "text": "Region", "styles": {}}]]}
                ]}, "children": []},
            {"type": "paragraph", "props": {}, "content": [
                {"type": "text", "text": "Signed in Brussels.", "styles": {}}
            ], "children": []}
        ])
    }

    #[test]
    fn a_document_reads_as_blocks_that_remember_where_they_came_from() {
        let read = Document::read(&document()).unwrap();
        assert_eq!(read.blocks.len(), 6, "the nested paragraph is a block too");
        let terms = &read.blocks[0];
        assert_eq!(terms.kind, "heading");
        assert_eq!(terms.level, Some(1));
        assert_eq!(terms.text, "Payment terms");
        assert_eq!(terms.position, 1);
        assert_eq!(terms.path, vec![0]);

        // Inline styling is flattened, so the sentence reads as a sentence.
        let due = &read.blocks[1];
        assert_eq!(due.text, "Invoices are due within 30 days");
        assert!(due.rewritable);

        // A nested block carries its depth and the path to itself.
        let annex = &read.blocks[3];
        assert_eq!(annex.depth, 1);
        assert_eq!(annex.path, vec![2, 0]);
        assert_eq!(annex.id.as_deref(), Some("b4"));

        // A table's words are readable and its structure is not rewritable.
        let table = &read.blocks[4];
        assert_eq!(table.text, "Region");
        assert!(!table.rewritable);

        // A block with no id is reported as having none rather than given one.
        assert_eq!(read.blocks[5].id, None);
        assert_eq!(read.words(), 23);
    }

    #[test]
    fn a_blob_that_is_not_a_document_says_which_kind_of_nothing() {
        assert_eq!(
            Document::read(&json!({"blocks": []})).unwrap_err(),
            DocError::NotADocument
        );
        assert_eq!(DocError::NotJson.as_str(), "notJson");
        // An empty document is a document: a new one is exactly that.
        assert!(Document::read(&json!([])).unwrap().blocks.is_empty());
    }

    #[test]
    fn a_block_is_addressed_by_its_own_id_and_a_position_only_as_a_fallback() {
        let read = Document::read(&document()).unwrap();
        assert_eq!(read.block("b2").unwrap().position, 2);
        assert_eq!(read.block("  b4 ").unwrap().depth, 1);
        assert_eq!(read.block("#6").unwrap().text, "Signed in Brussels.");
        assert_eq!(read.block("6").unwrap().id, None);
        assert!(read.block("b9").is_none());
        assert!(read.block("").is_none());
        assert!(read.block("99").is_none());
    }

    #[test]
    fn a_passage_is_cited_to_the_heading_it_sits_under() {
        let read = Document::read(&document()).unwrap();
        let due = read.block("b2").unwrap();
        assert_eq!(
            read.heading_above(due).map(|h| h.text.as_str()),
            Some("Payment terms")
        );
        // The heading itself sits under nothing, and neither does anything
        // above the first one.
        assert!(read.heading_above(read.block("b1").unwrap()).is_none());
    }

    #[test]
    fn a_question_finds_the_blocks_that_hold_its_words_best_first() {
        let read = Document::read(&document()).unwrap();
        let terms =
            crate::sheet_grid::search_terms("What are the payment terms for late invoices?");
        assert_eq!(terms, ["payment", "terms", "late", "invoices"]);
        let found = read.find(&terms, 5);
        assert_eq!(
            found.iter().map(|b| b.position).collect::<Vec<_>>(),
            [1, 3, 2],
            "the two blocks holding two of the words come first, in reading order"
        );
        assert!(read.find(&[], 5).is_empty());
        assert!(read.find(&["nothing".to_owned()], 5).is_empty());
    }

    /// The passage a heading names comes with it: the words asked about are in
    /// the heading, and the answer is underneath it.
    #[test]
    fn a_matched_heading_brings_the_section_beneath_it() {
        let read = Document::read(&document()).unwrap();
        let heading = read.block("b1").unwrap();
        assert_eq!(
            read.section_under(heading, 10)
                .iter()
                .map(|b| b.position)
                .collect::<Vec<_>>(),
            [2, 3, 4, 5, 6],
            "a level-1 heading owns everything until the next one"
        );
        assert_eq!(read.section_under(heading, 2).len(), 2, "and it is bounded");
        // A paragraph is not a section, and asking for its body is nothing.
        assert!(read.section_under(read.block("b2").unwrap(), 10).is_empty());
    }

    #[test]
    fn the_runs_of_one_sentence_are_joined_exactly_as_they_were_written() {
        // A bold phrase in the middle of a paragraph is three runs, and the
        // sentence must read back as the one the user typed — a double space
        // would be a sentence they cannot find by searching their own document.
        let read = Document::read(&json!([
            {"id": "b1", "type": "paragraph", "content": [
                {"type": "text", "text": "Invoices are due within ", "styles": {}},
                {"type": "text", "text": "30 days", "styles": {"bold": true}},
                {"type": "text", "text": " of issue.", "styles": {}}
            ], "children": []}
        ]))
        .unwrap();
        assert_eq!(
            read.blocks[0].text,
            "Invoices are due within 30 days of issue."
        );
        // …and a table's cells do not run into one another.
        let table = Document::read(&json!([
            {"id": "t", "type": "table", "content": {"type": "tableContent", "rows": [
                {"cells": [
                    [{"type": "text", "text": "North", "styles": {}}],
                    [{"type": "text", "text": "1200", "styles": {}}]
                ]}
            ]}, "children": []}
        ]))
        .unwrap();
        assert_eq!(table.blocks[0].text, "North 1200");
    }

    #[test]
    fn a_rewrite_replaces_one_block_and_leaves_the_rest_of_the_document_alone() {
        let mut raw = document();
        let read = Document::read(&raw).unwrap();
        let path = read.block("b2").unwrap().path.clone();
        set_text(&mut raw, &path, "Invoices are due within 14 days.").unwrap();

        let block = &raw[1];
        assert_eq!(block["type"], json!("paragraph"), "the type survives");
        assert_eq!(block["id"], json!("b2"));
        assert_eq!(
            block["content"],
            json!([{"type": "text", "text": "Invoices are due within 14 days.", "styles": {}}])
        );
        // Everything else is untouched, including the nested paragraph and the
        // table this rewrite did not name.
        assert_eq!(raw[0]["content"][0]["text"], json!("Payment terms"));
        assert_eq!(raw[2]["children"][0]["id"], json!("b4"));
        assert_eq!(raw[3]["content"]["type"], json!("tableContent"));
        assert!(Document::read(&raw).is_ok());
    }

    #[test]
    fn a_rewrite_keeps_the_styling_the_block_was_written_in() {
        // A block that was entirely bold stays bold: dropping the styling would
        // be a change to the document nobody asked for.
        let mut raw = json!([
            {"id": "b1", "type": "paragraph", "props": {}, "content": [
                {"type": "text", "text": "ALL BOLD", "styles": {"bold": true}}
            ], "children": []}
        ]);
        set_text(&mut raw, &[0], "still bold").unwrap();
        assert_eq!(raw[0]["content"][0]["styles"], json!({"bold": true}));
    }

    #[test]
    fn a_block_whose_text_is_a_structure_is_refused_by_name() {
        let mut raw = document();
        let read = Document::read(&raw).unwrap();
        let table = read.block("b5").unwrap().path.clone();
        assert_eq!(
            set_text(&mut raw, &table, "one sentence"),
            Err(WriteError::NotRewritable)
        );
        assert_eq!(
            set_text(&mut raw, &[99], "nowhere"),
            Err(WriteError::NoSuchBlock)
        );
        assert_eq!(
            set_text(&mut raw, &[], "nowhere"),
            Err(WriteError::NoSuchBlock)
        );
        assert_eq!(WriteError::NotRewritable.as_str(), "notRewritable");
        // …and the document is exactly as it was.
        assert_eq!(raw, document());
    }

    #[test]
    fn a_section_lands_beside_the_block_it_was_put_after() {
        let mut raw = document();
        let read = Document::read(&raw).unwrap();
        let after = read.block("b1").unwrap().path.clone();
        let at = insert_blocks(
            &mut raw,
            Some(&after),
            vec![
                new_block("n1", "heading", Some(2), "Late payment"),
                new_block("n2", "paragraph", None, "Interest is charged monthly."),
            ],
        )
        .unwrap();
        assert_eq!(at, 1);
        assert_eq!(raw.as_array().unwrap().len(), 7);
        assert_eq!(raw[1]["id"], json!("n1"));
        assert_eq!(raw[1]["props"]["level"], json!(2));
        assert_eq!(raw[2]["id"], json!("n2"));
        // The block it followed and the one that followed it are both intact
        // and in order.
        assert_eq!(raw[0]["id"], json!("b1"));
        assert_eq!(raw[3]["id"], json!("b2"));

        let read = Document::read(&raw).unwrap();
        assert_eq!(
            read.block("n2").unwrap().text,
            "Interest is charged monthly."
        );
        assert!(holds_id(&read, "n1"));
        assert!(!holds_id(&read, "n9"));
    }

    #[test]
    fn a_section_after_a_nested_block_stays_at_that_blocks_own_level() {
        let mut raw = document();
        let read = Document::read(&raw).unwrap();
        let after = read.block("b4").unwrap().path.clone();
        insert_blocks(
            &mut raw,
            Some(&after),
            vec![new_block("n1", "paragraph", None, "Beside it.")],
        )
        .unwrap();
        // Inside the list item's children, after b4 — not appended to the
        // document, and not made a child of b4.
        assert_eq!(raw[2]["children"][1]["id"], json!("n1"));
        assert_eq!(raw.as_array().unwrap().len(), 5);
    }

    #[test]
    fn nothing_named_appends_at_the_end_and_a_stranger_is_refused() {
        let mut raw = document();
        let at = insert_blocks(
            &mut raw,
            None,
            vec![new_block("n1", "paragraph", None, "Last.")],
        )
        .unwrap();
        assert_eq!(at, 5);
        assert_eq!(raw[5]["id"], json!("n1"));
        assert_eq!(
            insert_blocks(
                &mut raw,
                Some(&[42]),
                vec![new_block("n2", "paragraph", None, "x")]
            ),
            Err(WriteError::NoSuchBlock)
        );
        // A blob that is not an array cannot be written to and says so.
        let mut broken = json!({"blocks": []});
        assert_eq!(
            insert_blocks(&mut broken, None, Vec::new()),
            Err(WriteError::NoSuchBlock)
        );
    }

    #[test]
    fn a_new_block_carries_the_editors_own_defaults_and_nothing_else() {
        let paragraph = new_block("n1", "paragraph", None, "Words.");
        assert_eq!(paragraph["props"]["textColor"], json!("default"));
        assert_eq!(paragraph["props"]["textAlignment"], json!("left"));
        assert!(paragraph["props"].get("level").is_none());
        assert_eq!(paragraph["children"], json!([]));
        // A heading's level is clamped to what the editor offers.
        assert_eq!(
            new_block("n1", "heading", Some(9), "H")["props"]["level"],
            json!(3)
        );
        assert_eq!(
            new_block("n1", "heading", None, "H")["props"]["level"],
            json!(2)
        );
        // An empty block holds no empty run: BlockNote writes an empty list.
        assert_eq!(new_block("n1", "paragraph", None, "")["content"], json!([]));
        assert!(NEW_BLOCK_KINDS.contains(&"bulletListItem"));
        assert!(!NEW_BLOCK_KINDS.contains(&"table"));
    }
}
