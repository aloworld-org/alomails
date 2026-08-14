//! The **Drive** tool set of the agent (ADR 0034) — the names alo Drive
//! contributes to the one agent, and the words that tell a model what they
//! take.
//!
//! Everything here is a *proposal*. The model names a tool and its arguments;
//! the jmap layer executes it against the caller's tenant-scoped store, with
//! the caller's own reach and nothing more. A file the asker could not open is
//! a file the agent cannot find for them.
//!
//! Three rules shape the wording below, and each is a mistake it exists to
//! prevent:
//!
//! - **A file is found by its name, never by an id.** Ids are ours, not the
//!   user's; a model asked to produce one will invent a plausible string, and
//!   an invented id either resolves to nothing or — far worse — to somebody
//!   else's document. The name the user said is passed through verbatim.
//! - **Only what the system can actually do.** Sharing and summarising were
//!   drafted here and removed: neither capability exists in the store yet, and
//!   a tool the model may propose but nothing can perform is worse than an
//!   absent one — it produces a confident offer that ends in a failure the
//!   user cannot act on. They belong here the day Drive grows a sharing API
//!   and a text extractor, and not a day sooner.
//! - **Nothing is ever deleted.** There is no `delete_file` and there will not
//!   be one. A model that misreads "clear out the old drafts" must not be able
//!   to act on it, and no phrasing of a description makes that safe. Removal
//!   stays a human act in Drive itself.

use crate::agent_tool::AgentTool;

/// The Drive tools, each declaring whether it reads or writes (ADR 0047 §1).
///
/// One tool, and it reads. There is no `delete_file` and there will not be
/// one; nothing here can change a byte of anybody's Drive.
pub const DRIVE_TOOLS: &[AgentTool] = &[AgentTool::read("find_file")];

/// What each Drive tool takes, in the words the model reads.
///
/// Read-versus-write is declared in [`DRIVE_TOOLS`] and rendered into the
/// prompt from there (ADR 0047 §1), never restated here.
pub const DRIVE_TOOL_DOC: &str = "\
- find_file: find files in the user's Drive by what they are called or what they are about. args: {\"query\": string (what the user called it, in their own words, required), \"limit\": integer (optional, at most 20)}. Use this when the user asks where something is, or asks for a file by name. Pass their words through exactly — never tidy a filename, never guess an extension, and never invent an identifier.\n";

/// The rules that keep a Drive proposal honest, appended to the system prompt.
pub const DRIVE_GUIDANCE: &str = "For a Drive tool, pass the file's name through EXACTLY as the user gave it — never complete it, correct its spelling, add an extension, or supply an identifier of any kind. A file has no number the user knows, so it is not in the numbered sources. If the user did not name a file, ANSWER and ask which one they mean rather than searching for something plausible. You cannot delete anything in Drive and must not offer to; if the user asks you to remove a file, say that removal is theirs to do in Drive.\n";

#[cfg(test)]
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
    }

    /// Deletion is absent by design, not by oversight. If somebody adds it,
    /// this test is where the argument has to be had.
    #[test]
    fn nothing_here_can_destroy_a_file() {
        for tool in DRIVE_TOOLS {
            assert!(
                tool.is_read(),
                "{} would let a misread sentence destroy somebody's work",
                tool.name
            );
        }
        assert!(DRIVE_GUIDANCE.contains("cannot delete"));
    }

    /// A tool the model can name must be a tool something can perform. The
    /// workspace-wide guard checks this across every set; this one keeps the
    /// argument local to Drive.
    #[test]
    fn the_set_is_only_what_drive_can_actually_do() {
        assert_eq!(DRIVE_TOOLS, &[AgentTool::read("find_file")]);
    }

    /// The whole point of naming files rather than numbering them.
    #[test]
    fn the_model_is_told_to_pass_names_through_verbatim() {
        assert!(DRIVE_GUIDANCE.contains("EXACTLY"));
        assert!(DRIVE_TOOL_DOC.contains("never invent an identifier"));
    }
}
