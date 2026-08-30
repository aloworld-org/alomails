//! alo Docs' verbs (ADR 0058, queue item AB.2) — the whole of what the Docs
//! agent may do, and the words a model reads about it.
//!
//! Nothing here reads or writes a document: the executors live in `alo-jmap`
//! (`docs_intents.rs`, with the older tool executors it keeps in
//! `agent_docs.rs`), through the asker's tenant-scoped store — a document the
//! asker could not open is not among the things that can be named.
//!
//! **The documents are the ones in Drive.** The Docs editor's document is a
//! Drive node of kind `doc` whose blob is the editor's own block array; that
//! is what every verb below reads and writes, which is why no verb adapts a
//! `/docs/` route — those routes serve the standalone technical-authoring
//! surface (ADR 0015), a different record with its own screens, excluded
//! below with its reasons.
//!
//! The rules the hand-written tool set learned, kept because each one is a
//! mistake it exists to prevent:
//!
//! - **A passage is cited to the block it came from.** Every read hands back
//!   block ids and the heading each block sits under, and the guidance says
//!   to repeat them, because a person has to be able to find the sentence
//!   the agent is talking about.
//! - **A rewrite replaces words, never structure.** `doc_rewrite` puts the
//!   caller's own text into blocks that already exist; it cannot delete a
//!   block, move one, or change a heading into a paragraph. The one write
//!   that adds anything is `doc_draft_section`, and what it adds is plain
//!   blocks.
//! - **Translation is the same write, not a second mechanism.** A document is
//!   translated by reading it and proposing one `doc_rewrite` carrying every
//!   block's text in the new language. There is no separate translate tool to
//!   drift from the one that actually edits the document.
//! - **Nothing is ever deleted.** There is no way to remove a document, a
//!   block, or a word somebody wrote: a draft only adds, a rewrite only
//!   replaces text it was shown. Removal stays a human act in the editor.

use crate::agent_tool::Effect;
use crate::intent::{Arg, Excluded, IntentModule, IntentSpec};

const DOCUMENT_OPT: Arg = Arg::optional(
    "document",
    "text",
    "which document, by its name in Drive; the user's only one when left out",
);

/// The verbs.
pub const DOCS_INTENTS: &[IntentSpec] = &[
    // ---- reads ---------------------------------------------------------
    IntentSpec {
        name: "list_documents",
        purpose: "The documents of the user's own Drive, most recently edited first — each with its name and when it was last touched — or the documents inside one folder, by the folder's name. It changes nothing. What \"which documents exist\" means to a person looking at their Drive.",
        effect: Effect::Read,
        args: &[
            Arg::optional(
                "folder",
                "text",
                "a folder of the user's own Drive, by name; every document of theirs when left out",
            ),
            Arg::optional("limit", "integer", "at most 20"),
        ],
        answers: &[
            "which documents exist",
            "which documents do we have",
            "what did we write lately",
            "what documents are in the {folder} folder",
        ],
        preview: None,
        undo: None,
        routes: &[],
    },
    IntentSpec {
        name: "doc_read",
        purpose: "Read a document as it stands — its blocks in the order they read on the page, each with its own id, its kind (heading, paragraph, list item) and its text. It changes nothing. This is where the block ids every write needs come from: read before drafting, rewriting or translating anything, and never invent an id.",
        effect: Effect::Read,
        args: &[
            DOCUMENT_OPT,
            Arg::optional("from", "text", "the id of the block to start at"),
            Arg::optional("blocks", "integer", "how many blocks, at most 60"),
        ],
        answers: &[
            "what does the handover doc say",
            "read me the terms document",
            "summarise that document",
        ],
        preview: None,
        undo: None,
        routes: &[],
    },
    IntentSpec {
        name: "doc_answer",
        purpose: "The passages of a document that mention what the user asked about, each with its block id and the heading it sits under. It searches; it changes nothing. Answer from the passages it returns and say which section each came from; when it returns nothing, the honest answer is that the document does not say.",
        effect: Effect::Read,
        args: &[
            Arg::required(
                "question",
                "text",
                "what to look for, in the user's own words",
            ),
            DOCUMENT_OPT,
        ],
        answers: &[
            "what do we say about payment terms",
            "does the contract mention notice periods",
        ],
        preview: None,
        undo: None,
        routes: &[],
    },
    // ---- writes: propose, then the asker approves ------------------------
    IntentSpec {
        name: "create_document",
        purpose: "Make a new, empty document in the user's own Drive — at the top level, or inside one of their folders. A name a sibling already has is refused rather than made unique. It creates and nothing more: what the document should say is drafted afterwards, block by block, with doc_draft_section.",
        effect: Effect::Write,
        args: &[
            Arg::required("title", "text", "what the document should be called"),
            Arg::optional(
                "folder",
                "text",
                "a folder of the user's own Drive, by name; the top level when left out",
            ),
        ],
        answers: &[
            "start a document called {title}",
            "create a doc for the meeting notes",
        ],
        preview: Some("An empty document called {title} will be created in the user's own Drive."),
        undo: None,
        routes: &[],
    },
    IntentSpec {
        name: "doc_draft_section",
        purpose: "Propose NEW blocks to add to a document — a heading and the paragraphs under it, a list, a paragraph on its own. It only adds: it cannot delete or replace anything already written. Write in the user's own language and in the document's own voice, and say in your own words what is about to be added.",
        effect: Effect::Write,
        args: &[
            Arg::required(
                "blocks",
                "array",
                "the blocks to add, each {\"kind\": one of paragraph, heading, bulletListItem, numberedListItem, checkListItem, quote, \"text\": string, \"level\": a heading's level 1 to 3, optional}; at most 40",
            ),
            DOCUMENT_OPT,
            Arg::optional(
                "after",
                "text",
                "the id of the block to put them after, from doc_read; the end of the document when left out",
            ),
        ],
        answers: &[
            "draft a section on onboarding",
            "add a summary to the notes",
        ],
        preview: Some(
            "The drafted blocks will be added to the document — nothing already written is removed or replaced.",
        ),
        undo: None,
        routes: &[],
    },
    IntentSpec {
        name: "doc_rewrite",
        purpose: "Propose replacing the TEXT of blocks that already exist, one new text per block — how a selection is rewritten and how a document is translated. Each block keeps its kind, its level and its formatting; only the words change. A block whose content is a table or an image is refused by name. Never rewrite a block you have not read.",
        effect: Effect::Write,
        args: &[
            Arg::required(
                "blocks",
                "array",
                "the rewrites, each {\"block\": the block's id from doc_read or doc_answer, \"text\": the whole new text of that block}; at most 60",
            ),
            DOCUMENT_OPT,
        ],
        answers: &[
            "rewrite that paragraph more plainly",
            "translate the document into French",
        ],
        preview: Some(
            "The named blocks will carry the proposed wording — same blocks, same kinds, only the words change.",
        ),
        undo: None,
        routes: &[],
    },
];

/// The `/docs` routes deliberately without a verb, each with its reason.
///
/// These serve the standalone technical-authoring surface (ADR 0015), whose
/// documents are their own record — not the documents in Drive the Docs agent
/// works in. An agent verb over that surface would be a second document store
/// reachable under the same name.
pub const DOCS_EXCLUDED: &[Excluded] = &[
    Excluded {
        route: "/docs",
        why: "The standalone authoring surface (ADR 0015) keeps its own record apart from the documents in Drive the agent works in; a person reaches it in the app.",
    },
    Excluded {
        route: "/docs/{id}",
        why: "The same surface's single document — and the one route that can delete one, which is a person's deliberate act, never an agent's.",
    },
];

/// The Docs paragraph of the agent's general instructions.
pub const DOCS_GUIDANCE: &str = "For a document, ALWAYS say which part of it you are talking about — the heading a passage sits under, and the block id when you are about to change it — because a person has to be able to find the sentence you mean. Never answer about what a document says from memory or from a search snippet: read it first, and if a passage is not there, say plainly that the document does not say. To translate a document, read it and propose ONE rewrite carrying every block's text in the new language, keeping names, figures, dates and links exactly as they are; when a document is longer than one proposal can carry, say so and propose the part you have read rather than pretending the rest is done. Write in the user's own language and in the document's voice — you are drafting for them, not about them. You cannot delete a document or a block and must not offer to; if the user asks you to remove one, say that removal is theirs to do in the editor. Moving or renaming a document is the alo Drive agent's. Never say a document has been created or changed until the user has approved it.\n";

/// The module, as the registry reads it.
pub static DOCS: IntentModule = IntentModule {
    intents: DOCS_INTENTS,
    excluded: DOCS_EXCLUDED,
    guidance: DOCS_GUIDANCE,
};

#[cfg(test)]
mod tests {
    use super::*;

    /// No verb adapts a `/docs/` route, and that is the design rather than a
    /// gap: the agent's documents are Drive nodes reached through the store,
    /// and the `/docs/` surface (ADR 0015) is excluded whole.
    #[test]
    fn every_verb_has_a_purpose_and_a_question_and_no_authoring_route() {
        for intent in DOCS_INTENTS {
            assert!(
                intent.routes.is_empty(),
                "{} claims a route of the standalone surface",
                intent.name
            );
            assert!(
                intent.purpose.ends_with('.'),
                "{} purpose is not a sentence",
                intent.name
            );
            assert!(
                !intent.answers.is_empty(),
                "{} answers nothing",
                intent.name
            );
            if intent.effect == Effect::Write {
                assert!(
                    intent.preview.is_some(),
                    "{} is a write without a preview",
                    intent.name
                );
            }
        }
    }

    #[test]
    fn names_are_unique_and_the_doc_lists_each_once() {
        let mut names: Vec<&str> = DOCS_INTENTS.iter().map(|i| i.name).collect();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), DOCS_INTENTS.len());
        let doc = DOCS.doc();
        for intent in DOCS_INTENTS {
            assert_eq!(doc.matches(&format!("- {}:", intent.name)).count(), 1);
        }
        assert!(DOCS_GUIDANCE.ends_with('\n'));
    }

    #[test]
    fn an_exclusion_is_a_sentence_not_a_shrug() {
        for excluded in DOCS_EXCLUDED {
            assert!(
                excluded.why.ends_with('.'),
                "{}: {}",
                excluded.route,
                excluded.why
            );
            assert!(
                !DOCS_INTENTS
                    .iter()
                    .any(|i| i.routes.contains(&excluded.route)),
                "{} is both a verb's route and excluded",
                excluded.route
            );
        }
    }

    /// The two reads that ground an answer say plainly that they change
    /// nothing, because a question about a document answered with a button is
    /// the bug ADR 0047 was written about.
    #[test]
    fn the_reads_answer_and_only_the_writes_wait() {
        let find = |name: &str| DOCS.find(name).unwrap_or_else(|| panic!("{name}"));
        for reads in ["list_documents", "doc_read", "doc_answer"] {
            assert_eq!(find(reads).effect, Effect::Read, "{reads}");
            assert!(find(reads).purpose.contains("changes nothing"), "{reads}");
        }
        for changes in ["create_document", "doc_draft_section", "doc_rewrite"] {
            assert_eq!(find(changes).effect, Effect::Write, "{changes}");
        }
        assert!(DOCS_GUIDANCE.contains("Never say a document has been created or changed"));
    }

    /// The rule the whole set is shaped by: a passage is cited to the block
    /// and the section it came from, and an address is read rather than
    /// invented. A guessed block id is how a rewrite lands on the wrong
    /// paragraph.
    #[test]
    fn every_passage_is_cited_and_no_address_is_invented() {
        let find = |name: &str| DOCS.find(name).unwrap_or_else(|| panic!("{name}"));
        assert!(DOCS_GUIDANCE.contains("ALWAYS say which part of it you are talking about"));
        assert!(find("doc_read").purpose.contains("its own id"));
        assert!(find("doc_read").purpose.contains("never invent an id"));
        assert!(
            find("doc_answer")
                .purpose
                .contains("the heading it sits under")
        );
        assert!(
            find("doc_rewrite")
                .purpose
                .contains("Never rewrite a block you have not read")
        );
    }

    /// Translation is the rewrite, said where the model reads it. A second
    /// mechanism would be a second thing to keep honest, and the one that
    /// actually edits the document is this one.
    #[test]
    fn translation_is_the_same_write_and_carries_the_facts_across_unchanged() {
        let rewrite = DOCS
            .find("doc_rewrite")
            .unwrap_or_else(|| panic!("doc_rewrite"));
        assert!(rewrite.purpose.contains("how a document is translated"));
        assert!(DOCS_GUIDANCE.contains("propose ONE rewrite carrying every block's text"));
        assert!(
            DOCS_GUIDANCE.contains("keeping names, figures, dates and links exactly as they are"),
            "a translation that reworks the figures is a different document"
        );
        // And the honest answer when it does not fit, which is what stops a
        // half-translated document being reported as a translated one.
        assert!(DOCS_GUIDANCE.contains("longer than one proposal can carry"));
        for intent in DOCS_INTENTS {
            assert!(!intent.name.contains("translate"), "{}", intent.name);
        }
    }

    /// Deletion is absent by design, not by oversight — and the set does not
    /// grow a file verb by another name: where a document sits and what it is
    /// called are the Drive agent's to change.
    #[test]
    fn nothing_here_deletes_reorders_or_moves_a_document() {
        let find = |name: &str| DOCS.find(name).unwrap_or_else(|| panic!("{name}"));
        assert!(find("doc_draft_section").purpose.contains("It only adds"));
        assert!(
            find("doc_draft_section")
                .purpose
                .contains("cannot delete or replace anything already written")
        );
        assert!(
            find("doc_rewrite")
                .purpose
                .contains("only the words change")
        );
        assert!(find("doc_rewrite").purpose.contains("refused by name"));
        for intent in DOCS_INTENTS {
            assert!(
                !intent.name.contains("delete")
                    && !intent.name.contains("trash")
                    && !intent.name.contains("move")
                    && !intent.name.contains("rename"),
                "{} would be a way to lose or refile a document",
                intent.name
            );
        }
        assert!(DOCS_GUIDANCE.contains("cannot delete a document or a block"));
        assert!(DOCS_GUIDANCE.contains("Moving or renaming a document is the alo Drive agent's"));
    }
}
