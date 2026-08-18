//! What a campaign says — the alo Docs block model, held to what a mail client
//! can actually draw (alo Campaigns, ADR 0044, wave C3.1).
//!
//! Queue item C3.1: *content as the **Docs block model** — one editor, not a
//! second one.* That sentence is a decision about the product, not about
//! storage: alo already has a block editor people know
//! (`web/src/authoring/document.ts`, ADR 0015), and a campaign composer with its
//! own block vocabulary would be a second editor to learn, a second thing to
//! keep in step, and a second place every accessibility rule has to be
//! remembered. So the blocks here are the Docs blocks, tag for tag and field for
//! field: a `{"type":"paragraph","id":…,"text":…}` written by the Docs editor is
//! a campaign paragraph without translation.
//!
//! ## The envelope, and why there is one
//!
//! A document persists its blocks as a bare JSON array whose shape the caller
//! owns ([`crate::document`]). A campaign cannot: wave C3.2 compiles these
//! blocks into email-safe HTML with golden-file tests, so *the same blocks must
//! produce the same HTML* for as long as the golden files claim to mean
//! something. That needs a version somebody can read —
//! `{"schema_version": 1, "blocks": [ … ]}` — so a body written by a newer build
//! is refused by name rather than half-understood by an older renderer. The
//! envelope is this module's; everything inside it is Docs'.
//!
//! ## The one Docs block a campaign refuses, and why
//!
//! **`equation`.** KaTeX renders in a browser, and a mail client is not one:
//! Outlook draws through Word, and Apple Mail and Gmail strip the stylesheet a
//! formula needs. The only way to put mathematics in an email is to send a
//! picture of it, and a picture is exactly what half of recipients have blocked
//! (C3.5). Silently dropping the block would send a mail with a hole in the
//! argument; rendering the LaTeX source would send `\frac{a}{b}` to a customer.
//! So it is refused **at save time, by name, with the reason** — the person
//! composing can still say it in words, and finds that out while they are
//! writing rather than after the send.
//!
//! Every other Docs block is accepted, because every other one has an
//! email-safe rendering: headings and paragraphs are text, a table is the
//! layout primitive email is built out of anyway, and code is a monospace cell.
//!
//! ## What this module deliberately does not decide
//!
//! - **Images.** There is no image block, and its absence is a decision rather
//!   than an omission: an image in an email is a URL the *recipient's client*
//!   fetches from us, and a fetch that we can see is the open-tracking pixel ADR
//!   0044 refused to ship by default. Adding one needs that question answered —
//!   who may fetch, what is logged, what a blocked image leaves behind (C3.5) —
//!   and a new block variant is additive when it is.
//! - **What a merge field means.** `{{ref:…}}` is a section number in a
//!   document and nothing in an email; personalisation and its mandatory
//!   fallback are [`crate::campaign_merge`] (C3.4), which owns the grammar, the
//!   closed vocabulary and the resolution. This module *applies* that rule at
//!   the gate — a heading, a paragraph and a table cell are each checked as
//!   they are saved, so `Hi {{first_name}},` is refused while somebody is
//!   writing rather than delivered as `Hi ,` — and decides nothing about it.
//!   Text is still stored exactly as written: resolution happens per recipient,
//!   once, before either renderer.
//! - **How any of it looks.** This module is types and validation, with no
//!   database and no HTML in it. The renderer (C3.2) consumes exactly these
//!   types, so the vocabulary cannot drift between what is stored and what is
//!   drawn — the same split `site_model.rs` keeps for pages.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::campaign_merge::validate_merge_text;
use crate::error::{Result, StoreError};

/// The block-model version this build writes and reads.
///
/// A bump ships an explicit upgrade applied on read; stored JSON is rewritten on
/// the next save. Until then a body declaring another version is refused by
/// name — a renderer that half-read one would produce HTML nobody wrote.
pub const CAMPAIGN_CONTENT_SCHEMA_VERSION: u64 = 1;

/// The most blocks one campaign may hold. Far past any mail somebody will
/// actually read, and short of a paste that would take a renderer minutes.
pub const CAMPAIGN_BLOCKS_MAX: usize = 200;
/// Character cap for a heading.
pub const CAMPAIGN_HEADING_CHARS_MAX: usize = 300;
/// Character cap for a paragraph or a code block.
pub const CAMPAIGN_TEXT_CHARS_MAX: usize = 5_000;
/// The most rows one table may have, header included.
pub const CAMPAIGN_TABLE_ROWS_MAX: usize = 100;
/// The most columns one table may have. A mail is read on a phone; past this
/// the table stops being a table and starts being a horizontal scroll bar.
pub const CAMPAIGN_TABLE_COLUMNS_MAX: usize = 12;
/// Character cap for one table cell.
pub const CAMPAIGN_CELL_CHARS_MAX: usize = 300;
/// Character cap for a block id (a UUID from the editor, with room to spare).
pub const CAMPAIGN_BLOCK_ID_CHARS_MAX: usize = 64;
/// Character cap for a code block's language token.
pub const CAMPAIGN_LANGUAGE_CHARS_MAX: usize = 40;

/// A section heading. `level` is 1 or 2, as the Docs editor writes it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HeadingBlock {
    /// The editor's block id — stable across edits, unique within the body.
    pub id: String,
    /// 1 or 2. Deeper levels exist in documents and not in mail: an email with
    /// four levels of hierarchy is a document somebody should have linked to.
    pub level: u8,
    /// The heading itself. Never blank — an empty heading is a gap the reader
    /// sees and the writer does not.
    pub text: String,
}

/// Prose. The block a campaign is mostly made of.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ParagraphBlock {
    /// The editor's block id.
    pub id: String,
    /// The text, as written. **May be empty**: the Docs editor opens a new body
    /// with exactly one empty paragraph, and a model that refused its own
    /// editor's starting state would fail on the first keystroke.
    pub text: String,
}

/// A table. `rows[0]` is the header row, as in Docs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TableBlock {
    /// The editor's block id.
    pub id: String,
    /// The rows, header first. Rectangular: every row has the width of the
    /// header, because a ragged table cannot be drawn without column spans and
    /// a renderer guessing at them would misalign somebody's prices.
    pub rows: Vec<Vec<String>>,
}

/// A code sample, rendered as a monospace block.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CodeBlock {
    /// The editor's block id.
    pub id: String,
    /// The source, as written.
    pub code: String,
    /// The language token the editor chose (`typescript`, `rust`, …). Held to a
    /// plain token because C3.2 puts it in the rendered markup.
    pub language: String,
}

/// One block of a campaign body — the Docs vocabulary an email can carry.
///
/// The wire shape is the Docs shape: the variant tag is the `type` field, and
/// the payload's fields sit beside it (`{"type":"heading","id":…,"level":1,
/// "text":…}`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum CampaignBlock {
    /// A section heading.
    Heading(HeadingBlock),
    /// Prose.
    Paragraph(ParagraphBlock),
    /// A table.
    Table(TableBlock),
    /// A code sample.
    Code(CodeBlock),
}

impl CampaignBlock {
    /// This block's id — the handle a renderer anchors on and a validator
    /// checks for duplicates.
    pub fn id(&self) -> &str {
        match self {
            Self::Heading(block) => &block.id,
            Self::Paragraph(block) => &block.id,
            Self::Table(block) => &block.id,
            Self::Code(block) => &block.id,
        }
    }

    /// The wire tag, for an error message that names what was rejected.
    pub fn kind(&self) -> &'static str {
        match self {
            Self::Heading(_) => "heading",
            Self::Paragraph(_) => "paragraph",
            Self::Table(_) => "table",
            Self::Code(_) => "code",
        }
    }

    /// Checks this block's own rules.
    fn validate(&self) -> Result<()> {
        check_id(self.id())?;
        match self {
            Self::Heading(block) => {
                validate_merge_text("a heading", &block.text)?;
                if !(1..=2).contains(&block.level) {
                    return Err(invalid(
                        "a heading in a campaign is level 1 or 2 — deeper hierarchy than that \
                         belongs in a page somebody links to",
                    ));
                }
                if block.text.trim().is_empty() {
                    return Err(invalid(
                        "a heading with no text is a gap the reader sees and the writer does not",
                    ));
                }
                check_length("a heading", &block.text, CAMPAIGN_HEADING_CHARS_MAX)
            }
            Self::Paragraph(block) => {
                validate_merge_text("a paragraph", &block.text)?;
                check_length("a paragraph", &block.text, CAMPAIGN_TEXT_CHARS_MAX)
            }
            Self::Table(block) => validate_table(block),
            // A code block's `{{ … }}` is somebody else's template syntax and
            // stays literal — [`crate::campaign_merge`] records the reasoning,
            // and this is the one place a writer can put those braces in a
            // campaign at all.
            Self::Code(block) => {
                check_language(&block.language)?;
                check_length("a code block", &block.code, CAMPAIGN_TEXT_CHARS_MAX)
            }
        }
    }
}

/// A campaign body: the versioned envelope around the blocks.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CampaignContent {
    /// The block-model version this body is written in.
    pub schema_version: u64,
    /// The blocks, in reading order.
    pub blocks: Vec<CampaignBlock>,
}

impl CampaignContent {
    /// An empty body at the current version — a campaign nobody has written yet.
    pub fn empty() -> Self {
        CampaignContent {
            schema_version: CAMPAIGN_CONTENT_SCHEMA_VERSION,
            blocks: Vec::new(),
        }
    }

    /// Parses and fully validates a body. **This is the write gate**: nothing
    /// reaches the `campaigns` table without passing through here.
    ///
    /// The block *types* are checked before the shape, so a body carrying a
    /// formula is told about the formula rather than about a serde variant it
    /// never named — the difference between an error somebody can act on and
    /// one they file a support ticket about (`docs/design/ux-principles.md`).
    ///
    /// # Errors
    /// [`StoreError::Validation`] naming the violated rule: an unreadable
    /// envelope, a version this build does not speak, a block type an email
    /// cannot carry, a shape that is not a block, or a content rule (blank
    /// heading, ragged table, over-cap text, duplicate id).
    pub fn from_value(value: Value) -> Result<Self> {
        let version = value
            .get("schema_version")
            .and_then(Value::as_u64)
            .ok_or_else(|| {
                invalid(
                    "a campaign body is an object stating its schema_version and its blocks — \
                     a bare list of blocks does not say which model it is written in",
                )
            })?;
        if version != CAMPAIGN_CONTENT_SCHEMA_VERSION {
            return Err(invalid(format!(
                "this build writes campaign bodies at schema_version \
                 {CAMPAIGN_CONTENT_SCHEMA_VERSION}, and was given {version}"
            )));
        }
        check_block_types(&value)?;
        let content: Self = serde_json::from_value(value).map_err(|error| {
            invalid(format!(
                "a campaign block is missing a field or has one this model does not know: {error}"
            ))
        })?;
        content.validate()?;
        Ok(content)
    }

    /// Parses a body from stored or wire JSON text.
    ///
    /// # Errors
    /// [`StoreError::Validation`] on JSON that will not parse, then as
    /// [`from_value`](Self::from_value).
    pub fn parse(json: &str) -> Result<Self> {
        let value: Value = serde_json::from_str(json)
            .map_err(|error| invalid(format!("a campaign body must be JSON: {error}")))?;
        Self::from_value(value)
    }

    /// The body as the JSON text the column stores.
    ///
    /// # Errors
    /// [`StoreError::Validation`] — unreachable for values built from these
    /// types, but serialization is fallible by signature.
    pub fn to_json(&self) -> Result<String> {
        serde_json::to_string(self)
            .map_err(|error| invalid(format!("a campaign body could not be written: {error}")))
    }

    /// Content-rule validation: version, block count, every block's own rules,
    /// and the ids taken together.
    ///
    /// # Errors
    /// [`StoreError::Validation`] naming the violated rule.
    pub fn validate(&self) -> Result<()> {
        if self.schema_version != CAMPAIGN_CONTENT_SCHEMA_VERSION {
            return Err(invalid(format!(
                "this build writes campaign bodies at schema_version \
                 {CAMPAIGN_CONTENT_SCHEMA_VERSION}, and was given {}",
                self.schema_version
            )));
        }
        if self.blocks.len() > CAMPAIGN_BLOCKS_MAX {
            return Err(invalid(format!(
                "a campaign body holds at most {CAMPAIGN_BLOCKS_MAX} blocks"
            )));
        }
        for block in &self.blocks {
            block.validate()?;
        }
        check_ids_unique(&self.blocks)
    }
}

impl Default for CampaignContent {
    fn default() -> Self {
        Self::empty()
    }
}

/// The rejection every rule in this module returns.
fn invalid(detail: impl Into<String>) -> StoreError {
    StoreError::Validation(detail.into())
}

/// Names the block types this build cannot carry, before serde gets a chance to
/// call them "unknown variants".
///
/// `equation` earns a sentence of its own because it is a Docs block somebody
/// legitimately wrote and will be surprised to lose — see the module docs.
fn check_block_types(value: &Value) -> Result<()> {
    let Some(blocks) = value.get("blocks").and_then(Value::as_array) else {
        return Err(invalid(
            "a campaign body's blocks is a list, even when the campaign is empty",
        ));
    };
    for block in blocks {
        let Some(kind) = block.get("type").and_then(Value::as_str) else {
            return Err(invalid(
                "every block in a campaign body says which type it is",
            ));
        };
        match kind {
            "heading" | "paragraph" | "table" | "code" => {}
            "equation" => {
                return Err(invalid(
                    "a formula cannot be drawn by a mail client — the only way to send one is \
                     as a picture, and a picture is what half of recipients have blocked. Say \
                     it in words, or link to the page that shows it",
                ));
            }
            other => {
                return Err(invalid(format!(
                    "a campaign body has no block called {other:?} — it is written in the same \
                     blocks as a document: heading, paragraph, table, code"
                )));
            }
        }
    }
    Ok(())
}

/// Checks a block id: present, bounded, and a single opaque token.
fn check_id(id: &str) -> Result<()> {
    if id.trim().is_empty() {
        return Err(invalid(
            "every block in a campaign body carries an id the editor gave it",
        ));
    }
    if id.chars().count() > CAMPAIGN_BLOCK_ID_CHARS_MAX {
        return Err(invalid(format!(
            "a block id fits in {CAMPAIGN_BLOCK_ID_CHARS_MAX} characters"
        )));
    }
    if id.chars().any(|c| c.is_whitespace() || c.is_control()) {
        return Err(invalid(
            "a block id is one token — no spaces and no control characters",
        ));
    }
    Ok(())
}

/// Two blocks with one id is a copy-paste an editor made and a renderer cannot
/// resolve: anchors, and anything C3.2 later keys on a block, would silently
/// point at whichever came first.
fn check_ids_unique(blocks: &[CampaignBlock]) -> Result<()> {
    let mut seen = std::collections::HashSet::with_capacity(blocks.len());
    for block in blocks {
        if !seen.insert(block.id()) {
            return Err(invalid(format!(
                "two blocks in this campaign share the id {:?} — a block is identified by it, \
                 so a duplicate is a copy that would overwrite the original",
                block.id()
            )));
        }
    }
    Ok(())
}

/// Checks a string against a character cap, counting characters rather than
/// bytes so a body of accented text is not measured differently from an English
/// one — this is a European product.
fn check_length(what: &str, text: &str, max: usize) -> Result<()> {
    if text.chars().count() > max {
        return Err(invalid(format!("{what} fits in {max} characters")));
    }
    Ok(())
}

/// Checks a table: present, rectangular, bounded.
fn validate_table(block: &TableBlock) -> Result<()> {
    let Some(header) = block.rows.first() else {
        return Err(invalid(
            "a table with no rows is nothing to draw — a table starts with its header row",
        ));
    };
    if header.is_empty() {
        return Err(invalid("a table has at least one column"));
    }
    if header.len() > CAMPAIGN_TABLE_COLUMNS_MAX {
        return Err(invalid(format!(
            "a table in a mail has at most {CAMPAIGN_TABLE_COLUMNS_MAX} columns — most of it is \
             read on a phone"
        )));
    }
    if block.rows.len() > CAMPAIGN_TABLE_ROWS_MAX {
        return Err(invalid(format!(
            "a table in a mail has at most {CAMPAIGN_TABLE_ROWS_MAX} rows, header included"
        )));
    }
    for row in &block.rows {
        if row.len() != header.len() {
            return Err(invalid(format!(
                "every row of a table has the {} columns its header has — a ragged table cannot \
                 be drawn without guessing where the gaps go",
                header.len()
            )));
        }
        for cell in row {
            validate_merge_text("a table cell", cell)?;
            check_length("a table cell", cell, CAMPAIGN_CELL_CHARS_MAX)?;
        }
    }
    Ok(())
}

/// Checks a code block's language token — a plain identifier, because the
/// renderer puts it in markup.
fn check_language(language: &str) -> Result<()> {
    if language.trim().is_empty() {
        return Err(invalid(
            "a code block names its language, so the mail can be read as code",
        ));
    }
    if language.chars().count() > CAMPAIGN_LANGUAGE_CHARS_MAX {
        return Err(invalid(format!(
            "a language name fits in {CAMPAIGN_LANGUAGE_CHARS_MAX} characters"
        )));
    }
    if !language
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '-' | '#' | '.' | '_'))
    {
        return Err(invalid(
            "a language is a plain name like typescript or c++ — it is written into the mail's \
             markup, so it cannot carry anything else",
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;
    use serde_json::json;

    /// The body the Docs editor produces for a campaign nobody has typed into
    /// yet — one empty paragraph. It must be accepted, or the composer fails on
    /// the screen it opens with.
    fn starter() -> Value {
        json!({
            "schema_version": 1,
            "blocks": [{ "type": "paragraph", "id": "b1", "text": "" }],
        })
    }

    fn rejected(value: Value) -> String {
        match CampaignContent::from_value(value) {
            Err(StoreError::Validation(detail)) => detail,
            other => panic!("expected a validation error, got {other:?}"),
        }
    }

    #[test]
    fn the_editors_own_starting_state_is_a_valid_body() {
        let content = CampaignContent::from_value(starter()).expect("the starter body is valid");
        assert_eq!(content.blocks.len(), 1);
        assert_eq!(content.schema_version, CAMPAIGN_CONTENT_SCHEMA_VERSION);
        // And an empty campaign is a legitimate draft too.
        assert!(CampaignContent::empty().validate().is_ok());
    }

    #[test]
    fn a_docs_block_round_trips_through_the_stored_shape_unchanged() {
        let written = json!({
            "schema_version": 1,
            "blocks": [
                { "type": "heading", "id": "h1", "level": 1, "text": "Spring prices" },
                { "type": "paragraph", "id": "p1", "text": "Everything below is per litre." },
                { "type": "table", "id": "t1", "rows": [["Product", "Price"], ["Oil", "€12"]] },
                { "type": "code", "id": "c1", "code": "curl https://alo", "language": "bash" },
            ],
        });
        let content = CampaignContent::from_value(written.clone()).expect("valid body");
        let stored = content.to_json().expect("serialises");
        let read_back = CampaignContent::parse(&stored).expect("re-reads");
        assert_eq!(read_back, content, "a body must survive a save and a load");
        // The wire shape is the Docs shape — tag as `type`, fields beside it —
        // so a block written by the Docs editor needs no translation.
        assert_eq!(
            serde_json::from_str::<Value>(&stored).expect("json"),
            written,
            "the stored JSON is the JSON the editor wrote"
        );
    }

    #[test]
    fn a_formula_is_refused_by_name_rather_than_dropped_or_sent_as_latex() {
        let detail = rejected(json!({
            "schema_version": 1,
            "blocks": [{ "type": "equation", "id": "e1", "latex": "x^2", "numbered": true }],
        }));
        assert!(
            detail.contains("formula") && detail.contains("picture"),
            "the writer must be told why, not shown a serde variant: {detail}"
        );
    }

    #[test]
    fn a_block_type_this_build_does_not_know_names_itself() {
        let detail = rejected(json!({
            "schema_version": 1,
            "blocks": [{ "type": "carousel", "id": "x1" }],
        }));
        assert!(detail.contains("carousel"), "{detail}");
    }

    #[test]
    fn a_body_that_does_not_say_which_model_it_is_written_in_is_refused() {
        // A bare array is what `documents.blocks` stores; a campaign needs the
        // version, because C3.2's golden files are only meaningful against one.
        let detail = rejected(json!([{ "type": "paragraph", "id": "p1", "text": "" }]));
        assert!(detail.contains("schema_version"), "{detail}");
        let newer = rejected(json!({ "schema_version": 2, "blocks": [] }));
        assert!(newer.contains("schema_version"), "{newer}");
    }

    #[test]
    fn a_heading_is_never_blank_and_never_deeper_than_two() {
        let blank = rejected(json!({
            "schema_version": 1,
            "blocks": [{ "type": "heading", "id": "h1", "level": 1, "text": "  " }],
        }));
        assert!(blank.contains("heading"), "{blank}");
        let deep = rejected(json!({
            "schema_version": 1,
            "blocks": [{ "type": "heading", "id": "h1", "level": 3, "text": "Deep" }],
        }));
        assert!(deep.contains("level 1 or 2"), "{deep}");
    }

    #[test]
    fn a_ragged_table_is_refused_rather_than_guessed_at() {
        let detail = rejected(json!({
            "schema_version": 1,
            "blocks": [{
                "type": "table",
                "id": "t1",
                "rows": [["Product", "Price"], ["Oil"]],
            }],
        }));
        assert!(detail.contains("columns"), "{detail}");
        let empty = rejected(json!({
            "schema_version": 1,
            "blocks": [{ "type": "table", "id": "t1", "rows": [] }],
        }));
        assert!(empty.contains("header row"), "{empty}");
    }

    #[test]
    fn two_blocks_cannot_share_an_id() {
        let detail = rejected(json!({
            "schema_version": 1,
            "blocks": [
                { "type": "paragraph", "id": "same", "text": "one" },
                { "type": "paragraph", "id": "same", "text": "two" },
            ],
        }));
        assert!(detail.contains("same"), "{detail}");
    }

    #[test]
    fn an_unknown_field_on_a_known_block_is_refused_rather_than_kept() {
        // A body that carried a prop the renderer ignores is a body that looks
        // richer in the editor than it does in the inbox.
        let detail = rejected(json!({
            "schema_version": 1,
            "blocks": [{ "type": "paragraph", "id": "p1", "text": "hi", "colour": "red" }],
        }));
        assert!(
            detail.contains("colour") || detail.contains("unknown"),
            "{detail}"
        );
    }

    #[test]
    fn text_and_tables_are_bounded() {
        let long = "x".repeat(CAMPAIGN_TEXT_CHARS_MAX + 1);
        let detail = rejected(json!({
            "schema_version": 1,
            "blocks": [{ "type": "paragraph", "id": "p1", "text": long }],
        }));
        assert!(detail.contains("characters"), "{detail}");

        let wide: Vec<String> = (0..=CAMPAIGN_TABLE_COLUMNS_MAX)
            .map(|n| n.to_string())
            .collect();
        let detail = rejected(json!({
            "schema_version": 1,
            "blocks": [{ "type": "table", "id": "t1", "rows": [wide] }],
        }));
        assert!(detail.contains("columns"), "{detail}");

        let blocks: Vec<Value> = (0..=CAMPAIGN_BLOCKS_MAX)
            .map(|n| json!({ "type": "paragraph", "id": format!("p{n}"), "text": "" }))
            .collect();
        let detail = rejected(json!({ "schema_version": 1, "blocks": blocks }));
        assert!(detail.contains("blocks"), "{detail}");
    }

    #[test]
    fn a_merge_field_with_no_fallback_is_refused_wherever_the_writer_can_type_one() {
        // C3.4's rule, applied at this gate: the letter that would arrive as
        // "Hi ," cannot be saved. Every text-bearing block is covered, because
        // a rule that held in paragraphs and not in table cells would be found
        // by a customer's recipients.
        for block in [
            json!({ "type": "heading", "id": "b", "level": 1, "text": "For {{first_name}}" }),
            json!({ "type": "paragraph", "id": "b", "text": "Hi {{first_name}}," }),
            json!({ "type": "table", "id": "b", "rows": [["Who"], ["{{name}}"]] }),
        ] {
            let detail = rejected(json!({ "schema_version": 1, "blocks": [block.clone()] }));
            assert!(
                detail.contains("fallback"),
                "{block} must be refused for the missing fallback: {detail}"
            );
        }
        // Written with one, all three are ordinary bodies.
        assert!(
            CampaignContent::from_value(json!({
                "schema_version": 1,
                "blocks": [
                    { "type": "heading", "id": "h", "level": 1, "text": "For {{first_name|you}}" },
                    { "type": "paragraph", "id": "p", "text": "Hi {{first_name|there}}," },
                    { "type": "table", "id": "t", "rows": [["Who"], ["{{name|a customer}}"]] },
                ],
            }))
            .is_ok()
        );
    }

    #[test]
    fn a_code_block_keeps_its_braces_because_they_are_somebody_elses_template() {
        // Handlebars, Vue, Angular, Jinja and Go all write `{{ … }}`. A campaign
        // that could not carry a sample of one could not document any of them,
        // and this is the one place in a letter those braces stay literal.
        let content = CampaignContent::from_value(json!({
            "schema_version": 1,
            "blocks": [{
                "type": "code",
                "id": "c1",
                "code": "<p>{{ user.name }}</p>",
                "language": "html",
            }],
        }))
        .expect("a code sample of a template is a legal campaign body");
        assert_eq!(content.blocks.len(), 1);
    }

    #[test]
    fn a_language_is_a_plain_token_because_it_reaches_the_markup() {
        let detail = rejected(json!({
            "schema_version": 1,
            "blocks": [{
                "type": "code",
                "id": "c1",
                "code": "ok",
                "language": "\"><script>alert(1)</script>",
            }],
        }));
        assert!(detail.contains("plain name"), "{detail}");
        // The ordinary ones still pass, punctuation included.
        assert!(
            CampaignContent::from_value(json!({
                "schema_version": 1,
                "blocks": [{ "type": "code", "id": "c1", "code": "ok", "language": "c++" }],
            }))
            .is_ok()
        );
    }
}
