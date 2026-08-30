//! alo Drive's verbs (ADR 0058, queue item AB.1) — the whole of what the Drive
//! agent may do, and the words a model reads about it.
//!
//! Nothing here reads or writes a file: the executors live beside Drive's
//! routes in `alo-jmap` (`drive_intents.rs`, with the older tool executors it
//! keeps in `agent_drive.rs` and `agent_attachments.rs`), through the asker's
//! tenant-scoped store — a file the asker could not open is not among the
//! things that can be named.
//!
//! The rules the hand-written tool set learned, kept because each one is a
//! mistake it exists to prevent:
//!
//! - **A file is found by its name, never by an id.** Ids are ours, not the
//!   user's; a model asked to produce one invents a plausible string, and an
//!   invented id either resolves to nothing or — far worse — to somebody
//!   else's document.
//! - **Nothing is ever deleted.** There is no `delete_file` and there will not
//!   be one: a model that misreads "clear out the old drafts" must not be able
//!   to act on it. Removal stays a human act in Drive itself.
//! - **The writes change where a file is, what it is called, and which folders
//!   exist — never a byte inside a file, and never who can see one.** A move
//!   and a new folder stay inside the person's own Drive: `drive_move`
//!   re-scopes access (ADR 0027), so a destination in a Space would hand the
//!   file to everybody in it.
//! - **A summary is written from the file, or it is not written.** A file
//!   whose bytes are not text is refused by name and by what it is, because a
//!   plausible summary of a PDF nobody read is the failure mode `file_read`
//!   exists to close.
//!
//! Every `/drive/` route is either the adapter of a verb below or listed as
//! excluded with its reason; the coverage test in `alo-jmap` holds that.

use crate::agent_tool::Effect;
use crate::intent::{Arg, Excluded, IntentModule, IntentSpec};

const FILE_REQ: Arg = Arg::required(
    "file",
    "text",
    "what the file is called, exactly as the user said it",
);
const FOLDER_OPT: Arg = Arg::optional(
    "folder",
    "text",
    "a folder of the user's own Drive, by name; the top level when left out",
);
const CHARS_OPT: Arg = Arg::optional("chars", "integer", "how much text, at most 20000");

/// The verbs.
pub const DRIVE_INTENTS: &[IntentSpec] = &[
    // ---- reads ---------------------------------------------------------
    IntentSpec {
        name: "recent_files",
        purpose: "The files of the user's own Drive, most recently changed first — each with its name, kind, size and when it was last touched. What \"which files do we have\" means to a person looking at their Drive.",
        effect: Effect::Read,
        args: &[Arg::optional("limit", "integer", "at most 20")],
        answers: &[
            "which files do we have",
            "what is in my drive",
            "what did we work on lately",
            "what changed recently",
        ],
        preview: None,
        undo: None,
        routes: &["/drive/list"],
    },
    IntentSpec {
        name: "list_folder",
        purpose: "What one folder of the user's own Drive holds — its files and its subfolders, by the folder's name, or the top level when none is named.",
        effect: Effect::Read,
        args: &[FOLDER_OPT],
        answers: &[
            "what is in the {folder} folder",
            "list my drive",
            "what folders do I have",
        ],
        preview: None,
        undo: None,
        routes: &["/drive/list"],
    },
    IntentSpec {
        name: "shared_with_me",
        purpose: "What is shared with the user — the Spaces they belong to, each with what its files area holds. A Space is how files are shared in alo, so this is the whole of \"shared with me\".",
        effect: Effect::Read,
        args: &[Arg::optional(
            "space",
            "text",
            "limit to one Space, by its name",
        )],
        answers: &[
            "what is shared with me",
            "which spaces am I in",
            "what is in the {space} space",
        ],
        preview: None,
        undo: None,
        routes: &["/drive/list"],
    },
    IntentSpec {
        name: "find_file",
        purpose: "Find files in the user's own Drive by what they are called. Pass their words through exactly — never tidy a filename, never guess an extension, never invent an identifier.",
        effect: Effect::Read,
        args: &[
            Arg::required(
                "query",
                "text",
                "what the user called it, in their own words",
            ),
            Arg::optional("limit", "integer", "at most 20"),
        ],
        answers: &[
            "where is the handover note",
            "do we have a file called {query}",
        ],
        preview: None,
        undo: None,
        routes: &["/drive/list"],
    },
    IntentSpec {
        name: "file_read",
        purpose: "What a file in the user's Drive actually says, as running text — what a summary is written from; never summarise a file from its name or from a search result. It reads documents and plain text files; a spreadsheet, a picture, a PDF or an office file comes back refused BY NAME, and the honest answer then is that this file's contents cannot be read here.",
        effect: Effect::Read,
        args: &[FILE_REQ, CHARS_OPT],
        answers: &[
            "what does the handover note say",
            "summarise the notes",
            "what is in that file",
        ],
        preview: None,
        undo: None,
        routes: &["/drive/nodes/{id}"],
    },
    IntentSpec {
        name: "attachment_read",
        purpose: "What is attached to one of the user's emails — the list of its attachments, and the text of one of them. Name the email the way the user did; never invent a message identifier. An attachment whose bytes are not text is refused by name and type, and saying so is the answer.",
        effect: Effect::Read,
        args: &[
            Arg::required(
                "email",
                "text",
                "the subject of the email, or the words of it the user said",
            ),
            Arg::optional(
                "attachment",
                "text",
                "which attachment, by its filename; the list of them when left out",
            ),
            CHARS_OPT,
        ],
        answers: &[
            "what does the attachment say",
            "what did they attach to that email",
        ],
        preview: None,
        undo: None,
        // Reads a MIME part out of the caller's own mail — a surface no
        // `/drive/` route serves, which is why this verb adapts none.
        routes: &[],
    },
    // ---- writes: propose, then the asker approves ------------------------
    IntentSpec {
        name: "create_folder",
        purpose: "Make a new folder in the user's own Drive — at the top level, or inside one of their folders. A name a sibling already has is refused rather than made unique.",
        effect: Effect::Write,
        args: &[
            Arg::required("name", "text", "what the folder should be called"),
            FOLDER_OPT,
        ],
        answers: &["make a {name} folder", "create a folder for the invoices"],
        preview: Some("A folder called {name} will be created in the user's own Drive."),
        undo: None,
        routes: &["/drive/folders"],
    },
    IntentSpec {
        name: "file_rename",
        purpose: "A NEW NAME for a file in the user's Drive. It changes nothing inside the file and does not move it; the file keeps its extension whatever is proposed, so it still opens. A name another file in the same folder already has is refused by name.",
        effect: Effect::Write,
        args: &[
            FILE_REQ,
            Arg::required("name", "text", "what it should be called"),
        ],
        answers: &[
            "rename the draft to {name}",
            "call that file {name} instead",
        ],
        preview: Some("{file} will be renamed to {name} — same folder, same contents."),
        undo: None,
        routes: &["/drive/nodes/{id}"],
    },
    IntentSpec {
        name: "file_move",
        purpose: "Move a file into one of the folders of the user's OWN Drive — never into a Space or out of one, because that changes who can read it. A folder that is not there is refused, and the refusal lists the folders that are.",
        effect: Effect::Write,
        args: &[FILE_REQ, FOLDER_OPT],
        answers: &[
            "move the report into {folder}",
            "put that file back at the top of my drive",
        ],
        preview: Some(
            "{file} will move to {folder} in the user's own Drive — same name, same readers.",
        ),
        undo: None,
        routes: &["/drive/nodes/{id}/move"],
    },
];

/// The Drive routes deliberately without a verb, each with its reason.
pub const DRIVE_EXCLUDED: &[Excluded] = &[
    Excluded {
        route: "/drive/trash",
        why: "The trash is where a person changes their mind; an agent neither lists it nor empties it.",
    },
    Excluded {
        route: "/drive/files",
        why: "Takes an upload; a file arrives by a person putting it there.",
    },
    Excluded {
        route: "/drive/nodes/{id}/copy",
        why: "Copying a file is a later intent set.",
    },
    Excluded {
        route: "/drive/nodes/{id}/trash",
        why: "Nothing here deletes: a model that misreads \"clear out the old drafts\" must not be able to act on it. Removal stays a human act in Drive.",
    },
    Excluded {
        route: "/drive/nodes/{id}/restore",
        why: "Undoes a trashing the agent cannot propose; the trash stays a person's.",
    },
    Excluded {
        route: "/drive/nodes/{id}/versions",
        why: "Version history is a person's screen; a later intent set.",
    },
    Excluded {
        route: "/drive/nodes/{id}/versions/{no}/restore",
        why: "Rolling a file back to an earlier version is a person's deliberate correction.",
    },
    Excluded {
        route: "/drive/nodes/{id}/download",
        why: "Serves a file; file_read answers from the record instead.",
    },
    Excluded {
        route: "/drive/nodes/{id}/office",
        why: "Mints a Collabora editing token for a person's editor session.",
    },
    Excluded {
        route: "/drive/base",
        why: "alo Base's structured tables are their own surface (ADR 0032); a later intent set.",
    },
    Excluded {
        route: "/drive/base/{node}",
        why: "alo Base's structured tables are their own surface (ADR 0032); a later intent set.",
    },
    Excluded {
        route: "/drive/base/{node}/tables",
        why: "alo Base's structured tables are their own surface (ADR 0032); a later intent set.",
    },
    Excluded {
        route: "/drive/base-tables/{table}/fields",
        why: "alo Base's structured tables are their own surface (ADR 0032); a later intent set.",
    },
    Excluded {
        route: "/drive/base-tables/{table}/records",
        why: "alo Base's structured tables are their own surface (ADR 0032); a later intent set.",
    },
    Excluded {
        route: "/drive/base-tables/{table}/views",
        why: "alo Base's structured tables are their own surface (ADR 0032); a later intent set.",
    },
    Excluded {
        route: "/drive/base-records/{record}",
        why: "alo Base's structured tables are their own surface (ADR 0032); a later intent set.",
    },
];

/// The Drive paragraph of the agent's general instructions.
pub const DRIVE_GUIDANCE: &str = "For a Drive verb, pass the file's name through EXACTLY as the user gave it — never complete it, correct its spelling, add an extension, or supply an identifier of any kind. To answer a question about the files, USE a reading verb first and answer from what it returned. If the user did not name a file, ask which one they mean rather than searching for something plausible. Never say what a file or an attachment contains without reading it first: a name is not its contents, and a summary written from a filename is a guess presented as a fact. You cannot delete anything in Drive and must not offer to; if the user asks you to remove a file, say that removal is theirs to do in Drive. You cannot change what is written inside a file either — the alo Docs and alo Sheets agents are the ones that edit a document or a spreadsheet. Never say a folder has been created or a file renamed or moved until the user has approved it.\n";

/// The module, as the registry reads it.
pub static DRIVE: IntentModule = IntentModule {
    intents: DRIVE_INTENTS,
    excluded: DRIVE_EXCLUDED,
    guidance: DRIVE_GUIDANCE,
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_verb_has_a_route_a_purpose_and_a_question_it_answers() {
        for intent in DRIVE_INTENTS {
            // `attachment_read` reads the caller's mail, a surface no
            // `/drive/` route serves — the one verb that adapts no route.
            assert!(
                !intent.routes.is_empty() || intent.name == "attachment_read",
                "{} names no route",
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
        let mut names: Vec<&str> = DRIVE_INTENTS.iter().map(|i| i.name).collect();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), DRIVE_INTENTS.len());
        let doc = DRIVE.doc();
        for intent in DRIVE_INTENTS {
            assert_eq!(doc.matches(&format!("- {}:", intent.name)).count(), 1);
        }
        assert!(DRIVE_GUIDANCE.ends_with('\n'));
    }

    #[test]
    fn an_exclusion_is_a_sentence_not_a_shrug() {
        for excluded in DRIVE_EXCLUDED {
            assert!(
                excluded.why.ends_with('.'),
                "{}: {}",
                excluded.route,
                excluded.why
            );
            assert!(
                !DRIVE_INTENTS
                    .iter()
                    .any(|i| i.routes.contains(&excluded.route)),
                "{} is both a verb's route and excluded",
                excluded.route
            );
        }
    }

    /// Deletion is absent by design, not by oversight. If somebody adds it,
    /// this test is where the argument has to be had.
    #[test]
    fn nothing_here_can_destroy_a_file() {
        for intent in DRIVE_INTENTS {
            assert!(
                !intent.name.contains("delete")
                    && !intent.name.contains("trash")
                    && !intent.name.contains("purge"),
                "{} would let a misread sentence destroy somebody's work",
                intent.name
            );
        }
        assert!(DRIVE_GUIDANCE.contains("cannot delete"));
    }

    /// The one sentence that keeps a move from becoming a share: `drive_move`
    /// re-scopes a node's access (ADR 0027), so a move into a Space would hand
    /// the file to everybody in it — not a thing an agent proposes. And the
    /// set does not grow an editing verb by another name: a document is alo
    /// Docs' to edit and a spreadsheet alo Sheets'.
    #[test]
    fn a_write_never_changes_readers_or_bytes() {
        let find = |name: &str| DRIVE.find(name).unwrap_or_else(|| panic!("{name}"));
        assert!(find("file_move").purpose.contains("OWN Drive"));
        assert!(
            find("file_move")
                .purpose
                .contains("changes who can read it")
        );
        assert!(find("file_rename").purpose.contains("does not move it"));
        assert!(find("file_rename").purpose.contains("keeps its extension"));
        for intent in DRIVE_INTENTS {
            assert!(
                !intent.name.contains("edit") && !intent.name.contains("write"),
                "{} edits a file's contents, which is not Drive's",
                intent.name
            );
        }
        assert!(DRIVE_GUIDANCE.contains("cannot change what is written inside a file"));
    }
}
