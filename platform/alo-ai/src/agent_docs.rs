//! The **Docs** tool set of the agent (ADR 0034, queue item A2.3) — the names
//! alo Docs contributes to its own agent, and the words that tell a model what
//! they take.
//!
//! The same seam every product before it uses ([`crate::agent_sheets`]): a tool
//! list carrying each tool's effect, a description block, and a paragraph of
//! guidance. Nothing here reads or writes a document — the reading tools are
//! executed inside the turn and the writes only from an approval, both by
//! `alo-jmap`'s `agent_docs` over [`crate::doc_blocks`], against the caller's
//! own tenant-scoped store.
//!
//! Five rules shape the wording below, and each is a mistake it exists to
//! prevent:
//!
//! - **The editor's agent mode, addressable as an agent.** alo Doc already has
//!   "Ask AI" inside the editor (ADR 0029 §3), propose-then-approve. A2.3 does
//!   not build a second one: the same three things — draft, rewrite, translate —
//!   are reachable from a room, and they are still proposals.
//! - **A passage is cited to the block it came from.** Every read hands back
//!   block ids and the heading each block sits under, and the guidance says to
//!   repeat them, because a person has to be able to find the sentence the
//!   agent is talking about.
//! - **A rewrite replaces words, never structure.** `doc_rewrite` puts the
//!   caller's own text into blocks that already exist; it cannot delete a block,
//!   move one, or change a heading into a paragraph. The one write that adds
//!   anything is `doc_draft_section`, and what it adds is plain blocks.
//! - **Translation is the same write, not a second mechanism.** A document is
//!   translated by reading it and proposing one `doc_rewrite` carrying every
//!   block's text in the new language. There is no separate translate tool to
//!   drift from the one that actually edits the document, and no path that
//!   rewrites a document the user has not been shown a proposal for.
//! - **Both writes wait for a tap.** They change a document the user's
//!   colleagues are reading, so they are declared writes (ADR 0047 §1) and the
//!   only path that runs them is an approval the asker themselves gave.

use crate::agent_tool::AgentTool;

/// The Docs tools, each declaring whether it reads or writes (ADR 0047 §1).
///
/// The jmap layer validates an approved tool against the union of this list and
/// every other product's ([`crate::is_agent_tool`]) and owns the execution of
/// each.
pub const DOCS_TOOLS: &[AgentTool] = &[
    AgentTool::read("doc_read"),
    AgentTool::read("doc_answer"),
    AgentTool::write("doc_draft_section"),
    AgentTool::write("doc_rewrite"),
];

/// The description of each Docs tool, spliced into the agent's system prompt.
///
/// Every line ends with a newline so the block concatenates into the list above
/// it without the caller knowing how many tools Docs has.
pub const DOCS_TOOL_DOC: &str = "\
- doc_read: read a document as it stands — its blocks in the order they read on the page, each with its own id, its kind (heading, paragraph, list item) and its text. It changes nothing. args: {\"document\": string (which document, by its name in Drive, optional — their only one when left out), \"from\": string (the id of the block to start at, optional), \"blocks\": number (how many blocks, optional, at most 60)}. This is where the block ids every write needs come from: read before you draft, rewrite or translate anything, and never invent an id.\n\
- doc_answer: find the passages of a document that mention what the user asked about, returned with each block's id and the heading it sits under. It searches; it changes nothing. args: {\"question\": string (what to look for, in the user's own words, REQUIRED), \"document\": string (optional)}. Answer from the passages it returns, and say which section each came from. When it comes back with nothing, say you could not find it in the document rather than answering from anything else.\n\
- doc_draft_section: propose NEW blocks to add to a document — a heading and the paragraphs under it, a list, a paragraph on its own. It only adds: it cannot delete or replace anything already written. args: {\"blocks\": [{\"kind\": string (one of paragraph, heading, bulletListItem, numberedListItem, checkListItem, quote), \"text\": string, \"level\": number (a heading's level, 1 to 3, optional)}] (REQUIRED, at most 40), \"document\": string (optional), \"after\": string (the id of the block to put them after, optional — the end of the document when left out)}. Write the section in the user's own language and in the document's own voice, and say in your own words what you are about to add before proposing it.\n\
- doc_rewrite: propose replacing the TEXT of blocks that already exist, one new text per block. This is how a selection is rewritten and how a document is translated — read it, then propose the new wording of each block you are changing. args: {\"blocks\": [{\"block\": string (the block's id, from doc_read or doc_answer), \"text\": string (the whole new text of that block)}] (REQUIRED, at most 60), \"document\": string (optional)}. Each block keeps its kind, its level and its formatting; only the words change. A block whose content is a table or an image is refused by name — say so rather than dropping it silently. Never rewrite a block you have not read.\n";

/// The Docs paragraph of the agent's general instructions.
///
/// Kept apart from the per-tool lines above because it says what the *product*
/// is, not what a tool takes: it is the sentence that stops a model answering
/// about a document it has not opened, and the one that says how a translation
/// is proposed.
pub const DOCS_GUIDANCE: &str = "For a document, ALWAYS say which part of it you are talking about — the heading a passage sits under, and the block id when you are about to change it — because a person has to be able to find the sentence you mean. Never answer about what a document says from memory or from a search snippet: read it first, and if a passage is not there, say plainly that the document does not say. To translate a document, read it and propose ONE rewrite carrying every block's text in the new language, keeping names, figures, dates and links exactly as they are; when a document is longer than one proposal can carry, say so and propose the part you have read rather than pretending the rest is done. Write in the user's own language and in the document's voice — you are drafting for them, not about them. Never say a document has been changed until the user has approved it.\n";

#[cfg(test)]
#[allow(clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn every_document_tool_is_described_to_the_model() {
        for tool in DOCS_TOOLS {
            assert!(
                DOCS_TOOL_DOC.contains(&format!("- {}:", tool.name)),
                "{} has no description in the prompt",
                tool.name
            );
        }
        // …and nothing is described that cannot be executed.
        let described = DOCS_TOOL_DOC.matches("\n- ").count() + 1;
        assert_eq!(described, DOCS_TOOLS.len());
    }

    #[test]
    fn the_block_concatenates_cleanly_into_a_list() {
        assert!(DOCS_TOOL_DOC.ends_with('\n'));
        assert!(DOCS_TOOL_DOC.starts_with("- "));
        assert!(DOCS_GUIDANCE.ends_with('\n'));
    }

    fn line(name: &str) -> String {
        DOCS_TOOL_DOC
            .lines()
            .find(|line| line.starts_with(&format!("- {name}:")))
            .expect("the tool is described")
            .to_owned()
    }

    /// The two reads answer inside the turn; the two writes wait. Declared, not
    /// derived from the names — and the reads say plainly that they change
    /// nothing, because a question about a document answered with a button is
    /// the bug ADR 0047 was written about.
    #[test]
    fn the_reads_answer_and_only_the_two_that_change_the_document_wait() {
        for reads in ["doc_read", "doc_answer"] {
            assert!(crate::is_read_tool(reads), "{reads}");
            assert!(line(reads).contains("changes nothing"), "{reads}");
        }
        for changes in ["doc_draft_section", "doc_rewrite"] {
            assert!(!crate::is_read_tool(changes), "{changes}");
        }
        assert!(DOCS_GUIDANCE.contains("Never say a document has been changed"));
    }

    /// The rule the whole tool set is shaped by: a passage is cited to the
    /// block and the section it came from, and an address is read rather than
    /// invented. A guessed block id is how a rewrite lands on the wrong
    /// paragraph.
    #[test]
    fn every_passage_is_cited_and_no_address_is_invented() {
        assert!(DOCS_GUIDANCE.contains("ALWAYS say which part of it you are talking about"));
        assert!(line("doc_read").contains("its own id"));
        assert!(line("doc_read").contains("never invent an id"));
        assert!(line("doc_answer").contains("the heading it sits under"));
        assert!(line("doc_rewrite").contains("Never rewrite a block you have not read"));
    }

    /// Translation is the rewrite, said where the model reads it. A second
    /// mechanism would be a second thing to keep honest, and the one that
    /// actually edits the document is this one.
    #[test]
    fn translation_is_the_same_write_and_carries_the_facts_across_unchanged() {
        assert!(line("doc_rewrite").contains("how a document is translated"));
        assert!(DOCS_GUIDANCE.contains("propose ONE rewrite carrying every block's text"));
        assert!(
            DOCS_GUIDANCE.contains("keeping names, figures, dates and links exactly as they are"),
            "a translation that reworks the figures is a different document"
        );
        // And the honest answer when it does not fit, which is what stops a
        // half-translated document being reported as a translated one.
        assert!(DOCS_GUIDANCE.contains("longer than one proposal can carry"));
        // No tool in the set is a second translate path.
        for tool in DOCS_TOOLS {
            assert!(!tool.name.contains("translate"), "{}", tool.name);
        }
    }

    /// What the writes cannot do, stated where the model reads it. A tool that
    /// could delete a block would make an approved rewrite a way to lose
    /// somebody's work.
    #[test]
    fn nothing_here_deletes_or_reorders_a_document() {
        assert!(line("doc_draft_section").contains("It only adds"));
        assert!(
            line("doc_draft_section").contains("cannot delete or replace anything already written")
        );
        assert!(line("doc_rewrite").contains("only the words change"));
        assert!(line("doc_rewrite").contains("refused by name"));
        for tool in DOCS_TOOLS {
            assert!(
                !tool.name.contains("delete") && !tool.name.contains("move"),
                "{} would be a way to lose a document",
                tool.name
            );
        }
    }
}
