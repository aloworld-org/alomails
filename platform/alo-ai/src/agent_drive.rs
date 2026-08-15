//! The **Drive** tool set of the agent (ADR 0034, queue items A1.x and A2.5) —
//! the names alo Drive contributes to its own agent, and the words that tell a
//! model what they take.
//!
//! Everything here is a *proposal*. The model names a tool and its arguments;
//! the jmap layer executes it against the caller's tenant-scoped store, with
//! the caller's own reach and nothing more. A file the asker could not open is
//! a file the agent cannot find for them.
//!
//! Six rules shape the wording below, and each is a mistake it exists to
//! prevent:
//!
//! - **A file is found by its name, never by an id.** Ids are ours, not the
//!   user's; a model asked to produce one will invent a plausible string, and
//!   an invented id either resolves to nothing or — far worse — to somebody
//!   else's document. The name the user said is passed through verbatim.
//! - **Only what the system can actually do.** Sharing was drafted here and
//!   removed: no sharing API exists, and a tool the model may propose but
//!   nothing can perform is worse than an absent one — it produces a confident
//!   offer that ends in a failure the user cannot act on. `file_read` arrived
//!   the day there was something to read a file with, which is the same rule
//!   read forwards.
//! - **Reading a file is Drive's, and it hands back no addresses.** A2.5's
//!   "summarise a document" is answered by [`DRIVE_TOOLS`]'s `file_read`, not by
//!   a second copy of `doc_read`, and the two are different tools rather than
//!   the same tool twice: Docs reads a document *by block id* so that a rewrite
//!   can land on a paragraph, while Drive reads *a file* — any kind of file it
//!   can decode — as running text with no block ids in it at all. You cannot
//!   write anything from a `file_read`, and that is the point: Drive holds
//!   files, Docs edits documents.
//! - **Nothing is ever deleted.** There is no `delete_file` and there will not
//!   be one. A model that misreads "clear out the old drafts" must not be able
//!   to act on it, and no phrasing of a description makes that safe. Removal
//!   stays a human act in Drive itself.
//! - **The two writes change where a file is and what it is called — never a
//!   byte inside it, and never who can see it.** A move stays inside the
//!   person's own Drive: moving a file into a Space would hand it to everybody
//!   in that Space (`alo_store::AccountStore::drive_move` re-scopes access,
//!   ADR 0027), and re-drawing who can read something is not a thing an agent
//!   gets to propose.
//! - **Both writes wait for a tap** (ADR 0047 §1), and both are checked against
//!   the real Drive *before* they are carried out: a rename that would collide
//!   with a sibling, or a move into a folder that is not there, is refused **by
//!   name** rather than half-done.

use crate::agent_tool::AgentTool;

/// The Drive tools, each declaring whether it reads or writes (ADR 0047 §1).
///
/// Three reads and two writes. There is no `delete_file` and there will not be
/// one; nothing here can change a byte inside anybody's file, and nothing here
/// can change who is able to open one.
pub const DRIVE_TOOLS: &[AgentTool] = &[
    AgentTool::read("find_file"),
    AgentTool::read("file_read"),
    AgentTool::read("attachment_read"),
    AgentTool::write("file_rename"),
    AgentTool::write("file_move"),
];

/// What each Drive tool takes, in the words the model reads.
///
/// Read-versus-write is declared in [`DRIVE_TOOLS`] and rendered into the
/// prompt from there (ADR 0047 §1), never restated here.
pub const DRIVE_TOOL_DOC: &str = "\
- find_file: find files in the user's Drive by what they are called or what they are about. It changes nothing. args: {\"query\": string (what the user called it, in their own words, required), \"limit\": integer (optional, at most 20)}. Use this when the user asks where something is, or asks for a file by name. Pass their words through exactly — never tidy a filename, never guess an extension, and never invent an identifier.\n\
- file_read: read what a file in the user's Drive actually says, as running text. It changes nothing. args: {\"file\": string (what the user called it, in their own words, required), \"chars\": integer (how much text, optional, at most 20000)}. This is what a summary is written from: read the file, then say in your own words what is in it — never summarise a file from its name or from a search result. It reads documents and plain text files (notes, csv, markdown); a spreadsheet, a picture, a PDF or an office file comes back refused BY NAME, and the honest answer then is to say that this file's contents cannot be read here rather than to guess at them.\n\
- attachment_read: read what is attached to one of the user's emails — the list of its attachments, and the text of one of them. It changes nothing. args: {\"email\": string (the subject of the email, or the words of it the user said, required), \"attachment\": string (which attachment, by its filename, optional — the list of them when left out)}. Use this when the user asks what an attachment says or asks you to pull something out of one. Name the email the way the user did; never invent a message identifier. An attachment whose bytes are not text (a PDF, a picture, an office file) comes back refused by name and type, and saying so is the answer.\n\
- file_rename: propose a NEW NAME for a file in the user's Drive. It changes nothing inside the file and does not move it. args: {\"file\": string (what it is called now, required), \"name\": string (what it should be called, required)}. The file keeps its extension whatever you propose, so the file still opens; say the new name in full to the user before proposing it. A name another file in the same folder already has is refused by name rather than made unique for you.\n\
- file_move: propose moving a file into one of the folders of the user's own Drive. It changes nothing inside the file and does not rename it. args: {\"file\": string (what it is called, required), \"folder\": string (the folder to put it in, by name, optional — the top level of the Drive when left out)}. Only the person's own Drive: you cannot move anything into a Space or out of one, because that changes who can read it. A folder that is not there is refused, and the refusal lists the folders that are.\n";

/// The rules that keep a Drive proposal honest, appended to the system prompt.
pub const DRIVE_GUIDANCE: &str = "For a Drive tool, pass the file's name through EXACTLY as the user gave it — never complete it, correct its spelling, add an extension, or supply an identifier of any kind. A file has no number the user knows, so it is not in the numbered sources. If the user did not name a file, ANSWER and ask which one they mean rather than searching for something plausible. Never say what a file or an attachment contains without reading it first: a name is not its contents, and a summary written from a filename is a guess presented as a fact. You cannot delete anything in Drive and must not offer to; if the user asks you to remove a file, say that removal is theirs to do in Drive. You cannot change what is written inside a file either — renaming and moving are all you can propose, and the alo Docs and alo Sheets agents are the ones that edit a document or a spreadsheet. Never say a file has been renamed or moved until the user has approved it.\n";

#[cfg(test)]
#[allow(clippy::expect_used)]
mod tests {
    use super::*;

    /// The set is what the executor switches on; a name here with no arm there
    /// is a tool the model can propose and nothing can perform.
    #[test]
    fn every_named_tool_is_described() {
        for tool in DRIVE_TOOLS {
            assert!(
                DRIVE_TOOL_DOC.contains(&format!("- {}:", tool.name)),
                "{} is offered to the model with no description",
                tool.name
            );
        }
        // …and nothing is described that cannot be executed.
        let described = DRIVE_TOOL_DOC.matches("\n- ").count() + 1;
        assert_eq!(described, DRIVE_TOOLS.len());
    }

    #[test]
    fn the_block_concatenates_cleanly_into_a_list() {
        assert!(DRIVE_TOOL_DOC.ends_with('\n'));
        assert!(DRIVE_TOOL_DOC.starts_with("- "));
        assert!(DRIVE_GUIDANCE.ends_with('\n'));
    }

    fn line(name: &str) -> &'static str {
        DRIVE_TOOL_DOC
            .lines()
            .find(|line| line.starts_with(&format!("- {name}:")))
            .expect("the tool is described")
    }

    /// Deletion is absent by design, not by oversight. If somebody adds it,
    /// this test is where the argument has to be had.
    #[test]
    fn nothing_here_can_destroy_a_file() {
        for tool in DRIVE_TOOLS {
            assert!(
                !tool.name.contains("delete")
                    && !tool.name.contains("trash")
                    && !tool.name.contains("purge"),
                "{} would let a misread sentence destroy somebody's work",
                tool.name
            );
        }
        assert!(DRIVE_GUIDANCE.contains("cannot delete"));
    }

    /// ADR 0047 §1, said where the model reads it: the three reads answer
    /// inside the turn and say plainly that they change nothing; only the two
    /// that alter the Drive wait for a tap.
    #[test]
    fn the_reads_answer_and_only_the_two_that_change_the_drive_wait() {
        for reads in ["find_file", "file_read", "attachment_read"] {
            assert!(crate::is_read_tool(reads), "{reads}");
            assert!(line(reads).contains("changes nothing"), "{reads}");
        }
        for changes in ["file_rename", "file_move"] {
            assert!(!crate::is_read_tool(changes), "{changes}");
            assert!(line(changes).contains("propose"), "{changes}");
        }
        assert!(DRIVE_GUIDANCE.contains("Never say a file has been renamed or moved"));
    }

    /// The whole point of naming files rather than numbering them.
    #[test]
    fn the_model_is_told_to_pass_names_through_verbatim() {
        assert!(DRIVE_GUIDANCE.contains("EXACTLY"));
        assert!(line("find_file").contains("never invent an identifier"));
        assert!(line("attachment_read").contains("never invent a message identifier"));
    }

    /// A2.5's first rule: a summary is written from the file, and a file whose
    /// bytes cannot be read comes back refused rather than guessed at. A
    /// summary composed from a filename is the failure this wording exists to
    /// prevent.
    #[test]
    fn a_summary_is_written_from_the_file_and_never_from_its_name() {
        assert!(line("file_read").contains("This is what a summary is written from"));
        assert!(line("file_read").contains("never summarise a file from its name"));
        assert!(line("file_read").contains("refused BY NAME"));
        assert!(DRIVE_GUIDANCE.contains("a summary written from a filename is a guess"));
    }

    /// Drive reads a file; Docs reads a document by its block ids. Neither
    /// tool's description offers the other's addresses, which is what keeps
    /// `file_read` from becoming a second, driftable `doc_read`.
    #[test]
    fn reading_a_file_hands_back_no_address_a_write_could_use() {
        assert!(!line("file_read").contains("block"));
        assert!(!DRIVE_TOOL_DOC.contains("doc_read"));
        assert!(DRIVE_GUIDANCE.contains("cannot change what is written inside a file"));
        // And the set does not grow an editing tool by another name.
        for tool in DRIVE_TOOLS {
            assert!(
                !tool.name.contains("write") && !tool.name.contains("edit"),
                "{} edits a file's contents, which is not Drive's",
                tool.name
            );
        }
    }

    /// The one sentence that keeps a move from becoming a share. `drive_move`
    /// re-scopes a node's access (ADR 0027), so a move into a Space hands the
    /// file to everybody in it — which is not a thing an agent proposes.
    #[test]
    fn a_move_never_changes_who_can_read_the_file() {
        assert!(line("file_move").contains("Only the person's own Drive"));
        assert!(line("file_move").contains("changes who can read it"));
        assert!(line("file_rename").contains("does not move it"));
        assert!(line("file_move").contains("does not rename it"));
    }

    /// A rename that drops the extension is a file that stops opening, so the
    /// extension survives whatever the model proposes — and the model is told
    /// so, rather than being surprised by it after the tap.
    #[test]
    fn a_rename_cannot_make_a_file_stop_opening() {
        assert!(line("file_rename").contains("keeps its extension"));
        assert!(line("file_rename").contains("still opens"));
    }
}
